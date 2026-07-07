import { HermesClient } from '@pythnetwork/hermes-client'
import { PythSolanaReceiver } from '@pythnetwork/pyth-solana-receiver'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction, VersionedTransaction } from '@solana/web3.js'
import { useQueryClient } from '@tanstack/react-query'
import { useCallback, useState } from 'react'
import { bytesToFeedIdHex, hermesId } from '../../config/pythFeeds'
import { connection, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signV1WithSession } from '../../lib/transactions'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

export type FeedSide = 'collateral' | 'lend'

export interface FeedRefreshContext {
    /** Address of the existing Pyth-source `feed` account. */
    feed: PublicKey
    /** Feed IDs stored on-chain (`config.collateral_feed_id`, `config.lend_feed_id`). */
    collateralFeedIdBytes: number[]
    lendFeedIdBytes: number[]
    /** Mint pair for cache invalidation — same key the finder uses. */
    collateralMint: PublicKey
    lendMint: PublicKey
}

export interface SetFeedFromPythSplit {
    /** Ephemeral `PriceUpdateV2` account pubkey after a successful load, per side. */
    collateralAccount: PublicKey | null
    lendAccount: PublicKey | null
    /** Which action is currently in flight, or `null`. Also blocks other actions. */
    busy: FeedSide | 'commit' | null
    /** Transaction signatures produced by each phase. */
    signatures: {
        collateral: string[]
        lend: string[]
        commit: string[]
    }
    /** Last error surfaced to the UI. */
    error: string | null

    /** Post a fresh Pyth update for one side. Fires 1–2 sub-transactions. */
    loadSide: (side: FeedSide, ctx: FeedRefreshContext) => Promise<void>
    /**
     * Consume both loaded side updates via `set_from_pyth`, then close the
     * two ephemeral accounts to reclaim rent. Requires both sides loaded.
     */
    commit: (ctx: FeedRefreshContext) => Promise<void>
    /** Clear loaded pubkeys and signatures without touching on-chain state. */
    reset: () => void
}

/**
 * Horizontally split Pyth refresh flow. Instead of one bundled push, this
 * hook exposes three independent actions:
 *
 * - `loadSide('collateral', ctx)` — fetch and post an update for the
 *   collateral feed only (1–2 txs depending on VAA size).
 * - `loadSide('lend', ctx)` — same for the lend feed.
 * - `commit(ctx)` — call `set_from_pyth` referencing the two loaded ephemeral
 *   `PriceUpdateV2` accounts, then close them for rent (1 tx).
 *
 * The ephemeral post-update accounts persist on-chain between the load steps
 * and the commit, so refreshing just one side that has drifted is 2 txs
 * total instead of the full 3-tx bundle.
 *
 * `set_from_pyth` requires no signer beyond the payer, so any connected
 * wallet can drive this flow against any Pyth feed.
 */
export function useSetFeedFromPyth(): SetFeedFromPythSplit {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    const [collateralAccount, setCollateralAccount] = useState<PublicKey | null>(null)
    const [lendAccount, setLendAccount] = useState<PublicKey | null>(null)
    const [busy, setBusy] = useState<FeedSide | 'commit' | null>(null)
    const [signatures, setSignatures] = useState<{
        collateral: string[]
        lend: string[]
        commit: string[]
    }>({ collateral: [], lend: [], commit: [] })
    const [error, setError] = useState<string | null>(null)

    const reset = useCallback(() => {
        setCollateralAccount(null)
        setLendAccount(null)
        setBusy(null)
        setSignatures({ collateral: [], lend: [], commit: [] })
        setError(null)
    }, [])

    const loadSide = useCallback(
        async (side: FeedSide, ctx: FeedRefreshContext): Promise<void> => {
            if (!connected || !wallet?.signTransaction) {
                setError('Wallet not connected')
                return
            }
            if (busy !== null) {
                setError('Another action is in flight')
                return
            }
            setBusy(side)
            setError(null)
            try {
                const payer = new PublicKey(wallet.account.publicKey)
                const feedIdBytes =
                    side === 'collateral' ? ctx.collateralFeedIdBytes : ctx.lendFeedIdBytes
                const feedIdHex = bytesToFeedIdHex(feedIdBytes)

                // Independent Hermes fetch: one VAA covering just this side.
                const res = await hermes.getLatestPriceUpdates([hermesId(feedIdHex)], {
                    encoding: 'base64',
                })
                const updateData = res.binary.data
                if (!updateData?.length) throw new Error('Hermes returned no price update data')

                const receiver = new PythSolanaReceiver({
                    connection,
                    wallet: makeAnchorWallet(payer, wallet),
                })

                // Get the post instructions + the ephemeral account pubkey we can
                // reference later during commit. Close ixs are ignored — we rebuild
                // them in `commit()` from the stored account pubkey.
                const built = await receiver.buildPostPriceUpdateInstructions(updateData)
                const account = built.priceFeedIdToPriceUpdateAccount[hermesId(feedIdHex)]
                if (!account) throw new Error('Receiver did not return an update account')

                const txs = await receiver.batchIntoVersionedTransactions(built.postInstructions, {
                    computeUnitPriceMicroLamports: 50_000,
                })
                const sigs = await receiver.provider.sendAll!(txs)

                if (side === 'collateral') setCollateralAccount(account)
                else setLendAccount(account)
                setSignatures((prev) => ({ ...prev, [side]: sigs }))
            } catch (e) {
                setError(e instanceof Error ? e.message : String(e))
            } finally {
                setBusy(null)
            }
        },
        [busy, connected, wallet],
    )

    const commit = useCallback(
        async (ctx: FeedRefreshContext): Promise<void> => {
            if (!connected || !wallet?.signTransaction) {
                setError('Wallet not connected')
                return
            }
            if (!collateralAccount || !lendAccount) {
                setError('Load both sides before committing')
                return
            }
            if (busy !== null) {
                setError('Another action is in flight')
                return
            }
            setBusy('commit')
            setError(null)
            try {
                const payer = new PublicKey(wallet.account.publicKey)
                const receiver = new PythSolanaReceiver({
                    connection,
                    wallet: makeAnchorWallet(payer, wallet),
                })

                const setFromPyth = await feedProgram.methods
                    .setFromPyth()
                    .accountsPartial({
                        feed: ctx.feed,
                        collateralPriceUpdate: collateralAccount,
                        lendPriceUpdate: lendAccount,
                    })
                    .instruction()

                const closeCollateral =
                    await receiver.buildClosePriceUpdateInstruction(collateralAccount)
                const closeLend = await receiver.buildClosePriceUpdateInstruction(lendAccount)

                const txs = await receiver.batchIntoVersionedTransactions(
                    [
                        { instruction: setFromPyth, signers: [] },
                        closeCollateral,
                        closeLend,
                    ],
                    { computeUnitPriceMicroLamports: 50_000 },
                )
                const sigs = await receiver.provider.sendAll!(txs)

                setSignatures((prev) => ({ ...prev, commit: sigs }))
                setCollateralAccount(null)
                setLendAccount(null)
                queryClient.invalidateQueries({
                    queryKey: queryKeys.feeds.byPair(
                        ctx.collateralMint.toBase58(),
                        ctx.lendMint.toBase58(),
                    ),
                })
            } catch (e) {
                setError(e instanceof Error ? e.message : String(e))
            } finally {
                setBusy(null)
            }
        },
        [busy, collateralAccount, connected, lendAccount, queryClient, wallet],
    )

    return {
        collateralAccount,
        lendAccount,
        busy,
        signatures,
        error,
        loadSide,
        commit,
        reset,
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

type Session = NonNullable<ReturnType<typeof useWalletConnection>['wallet']>

/**
 * Minimal `@coral-xyz/anchor` `Wallet` backed by the connected wallet session.
 * The Pyth receiver's `AnchorProvider` uses this to sign the versioned
 * transactions its builder produces.
 */
function makeAnchorWallet(publicKey: PublicKey, session: Session) {
    const wallet = {
        publicKey,
        signTransaction: <T extends Transaction | VersionedTransaction>(tx: T) =>
            signV1WithSession(tx, session),
        signAllTransactions: <T extends Transaction | VersionedTransaction>(txs: T[]) =>
            Promise.all(txs.map((tx) => signV1WithSession(tx, session))),
    }
    return wallet as unknown as ConstructorParameters<typeof PythSolanaReceiver>[0]['wallet']
}

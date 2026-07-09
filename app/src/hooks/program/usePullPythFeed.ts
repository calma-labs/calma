import { HermesClient } from '@pythnetwork/hermes-client'
import { PythSolanaReceiver } from '@pythnetwork/pyth-solana-receiver'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction, VersionedTransaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { noRules } from '../../config/feedRules'
import {
    feedIdToBytes,
    hermesId,
    MAX_PYTH_AGE_SECS,
    USDC_USD_FEED_ID,
    type PythFeed,
} from '../../config/pythFeeds'
import { connection, feedPda, feedProgram } from '../../lib/program'
import { signV1WithSession } from '../../lib/transactions'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

/** True when two `[u8; 32]` feed ids (as number[]) are byte-equal. */
function bytesEqual(a: number[], b: number[]): boolean {
    return a.length === b.length && a.every((v, i) => v === b[i])
}

export interface PullPythFeedResult {
    /** Whether a new feed account was created for this wallet. */
    created: boolean
    /** Signatures of the posted transactions. */
    signatures: string[]
}

/**
 * Posts a live Pyth price for `feed` to the connected wallet's `feed` account
 * via the Pyth **Pull** flow:
 *   1. Ensure the wallet's feed PDA exists and is a Pyth feed bound to
 *      (selected asset, USDC/USD). Create it if missing.
 *   2. Fetch signed price updates from Hermes for both feed ids.
 *   3. Post the `PriceUpdateV2` accounts and call `set_from_pyth` referencing
 *      them, batched into versioned transactions signed by the wallet.
 */
export interface PullPythFeedInput {
    feed: PythFeed
    /**
     * SPL mint the feed is bound to on the collateral side. Included in the
     * feed PDA seeds, so the same wallet can hold multiple feeds keyed by
     * mint pair. Must be a real on-chain `Mint` account.
     */
    collateralMint: PublicKey
    /** SPL mint on the lend side (typically the base pool's lend mint). */
    lendMint: PublicKey
}

export function usePullPythFeed() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            feed,
            collateralMint,
            lendMint,
        }: PullPythFeedInput): Promise<PullPythFeedResult> => {
            if (!connected || !wallet?.signTransaction) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)
            const feedAccount = feedPda(payer, collateralMint, lendMint)

            const collBytes = feedIdToBytes(feed.id)
            const lendBytes = feedIdToBytes(USDC_USD_FEED_ID)

            // ── 1. Ensure a compatible feed exists ──────────────────────────────
            // The generated `Feed` type and the runtime `.account` namespace
            // disagree on casing, so decode with the coder directly instead.
            const feedInfo = await connection.getAccountInfo(feedAccount)
            const existing = feedInfo
                ? (feedProgram.coder.accounts.decode('feed', feedInfo.data) as {
                      config: {
                          source: Record<string, unknown>
                          collateralFeedId: number[]
                          lendFeedId: number[]
                      }
                  })
                : null
            let created = false

            if (existing) {
                const source = existing.config.source as Record<string, unknown>
                if (!('pyth' in source)) {
                    throw new Error(
                        "This mint pair's feed is a Manual feed — it can't accept Pyth updates.",
                    )
                }
                const boundColl = existing.config.collateralFeedId as number[]
                const boundLend = existing.config.lendFeedId as number[]
                if (!bytesEqual(boundColl, collBytes) || !bytesEqual(boundLend, lendBytes)) {
                    throw new Error(
                        `A feed for this mint pair is already bound to a different Pyth id. Feeds are immutable once created — use a different mint pair to price ${feed.symbol}.`,
                    )
                }
            } else {
                const createIx = await feedProgram.methods
                    .create({ pyth: {} }, collBytes, lendBytes, MAX_PYTH_AGE_SECS, noRules())
                    .accounts({
                        authority: payer,
                        collateralMint,
                        lendMint,
                        payer,
                    })
                    .instruction()

                const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
                const createTx = new Transaction({
                    blockhash,
                    lastValidBlockHeight,
                    feePayer: payer,
                }).add(createIx)
                const createSig = await signAndSend(createTx, wallet)
                await connection.confirmTransaction(
                    { signature: createSig, blockhash, lastValidBlockHeight },
                    'confirmed',
                )
                created = true
            }

            // ── 2. Fetch signed price updates from Hermes ───────────────────────
            const res = await hermes.getLatestPriceUpdates(
                [hermesId(feed.id), hermesId(USDC_USD_FEED_ID)],
                { encoding: 'base64' },
            )
            const updateData = res.binary.data
            if (!updateData?.length) throw new Error('Hermes returned no price update data')

            // ── 3. Post updates + set_from_pyth ─────────────────────────────────
            const receiver = new PythSolanaReceiver({
                connection,
                wallet: makeAnchorWallet(payer, wallet),
            })

            const builder = receiver.newTransactionBuilder({ closeUpdateAccounts: true })
            await builder.addPostPriceUpdates(updateData)
            await builder.addPriceConsumerInstructions(async (getPriceUpdateAccount) => {
                // `feed`'s PDA seed reads `feed.config.authority` from account
                // data, so Anchor can't auto-derive it — pass it explicitly.
                const ix = await feedProgram.methods
                    .setFromPyth()
                    .accountsPartial({
                        feed: feedAccount,
                        collateralPriceUpdate: getPriceUpdateAccount(hermesId(feed.id)),
                        lendPriceUpdate: getPriceUpdateAccount(hermesId(USDC_USD_FEED_ID)),
                    })
                    .instruction()
                return [{ instruction: ix, signers: [] }]
            })

            const txs = await builder.buildVersionedTransactions({
                computeUnitPriceMicroLamports: 50_000,
            })
            const signatures = await receiver.provider.sendAll(txs)

            return { created, signatures }
        },
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['feed-account'] })
        },
    })
}

// ── helpers ─────────────────────────────────────────────────────────────────

type Session = NonNullable<ReturnType<typeof useWalletConnection>['wallet']>

async function signAndSend(tx: Transaction, session: Session): Promise<string> {
    const signed = await signV1WithSession(tx, session)
    return connection.sendRawTransaction(signed.serialize(), { skipPreflight: false })
}

/**
 * A minimal `@coral-xyz/anchor` `Wallet` backed by the connected wallet session.
 * The Pyth receiver's internal `AnchorProvider` uses this to sign and send the
 * versioned transactions it builds.
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

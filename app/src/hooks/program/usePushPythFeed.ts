import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { noRules } from '../../config/feedRules'
import { DEFAULT_PRICE_TTL_MS, MAX_PYTH_AGE_MS } from '../../config/pythFeeds'
import { connection, feedPda, feedProgram } from '../../lib/program'
import { signV1WithSession } from '../../lib/transactions'

export interface PushPythFeedInput {
    /** Sponsored `PriceUpdateV2` account pubkey for the collateral leg. */
    collateralPushAccount: PublicKey
    /** Sponsored `PriceUpdateV2` account pubkey for the lend leg. */
    lendPushAccount: PublicKey
    /**
     * SPL mint the feed is bound to on the collateral side. Included in the
     * feed PDA seeds — the same wallet can hold many feeds keyed by mint pair.
     * Must be a real on-chain `Mint`.
     */
    collateralMint: PublicKey
    /** SPL mint on the lend side. */
    lendMint: PublicKey
}

export interface PushPythFeedResult {
    /** Whether a new feed account was created for this wallet. */
    created: boolean
    /** Signatures of the posted transactions. */
    signatures: string[]
}

/** True when two `[u8; 32]` feed ids (as number[]) are byte-equal. */
function bytesEqual(a: number[], b: number[]): boolean {
    return a.length === b.length && a.every((v, i) => v === b[i])
}

/** Encode a `PublicKey` as the 32-byte array the `feed` program expects in `feed_id` slots. */
function pubkeyToBytes(pk: PublicKey): number[] {
    return Array.from(pk.toBytes())
}

/**
 * Consume a **sponsored Pyth push feed** for this wallet's `feed` account.
 *
 * Unlike the pull hook (`usePullPythFeed`), this does not fetch a Hermes VAA
 * or post any ephemeral `PriceUpdateV2`. The two accounts are fixed-address,
 * continuously-refreshed `PriceUpdateV2` accounts maintained by the Pyth Data
 * Association ("sponsored push feeds"). On-chain, `set_from_pyth_push` pins to
 * these account **pubkeys** stored in the feed config.
 *
 * The flow:
 *   1. If no feed exists for this wallet, create one as `PythPush` bound to
 *      the two sponsored account pubkeys.
 *   2. If a feed already exists, verify it's `PythPush` and bound to the same
 *      pair (feeds are immutable once created).
 *   3. Call `set_from_pyth_push` referencing the two sponsored accounts.
 *
 * Availability: sponsored push feeds only exist on mainnet-beta. Callers should
 * gate this hook to that cluster.
 */
export function usePushPythFeed() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            collateralPushAccount,
            lendPushAccount,
            collateralMint,
            lendMint,
        }: PushPythFeedInput): Promise<PushPythFeedResult> => {
            if (!connected || !wallet?.signTransaction) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)
            const feedAccount = feedPda(collateralMint, lendMint)

            const collBytes = pubkeyToBytes(collateralPushAccount)
            const lendBytes = pubkeyToBytes(lendPushAccount)

            // ── 1. Ensure a compatible feed exists ──────────────────────────────
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
                if (!('pythPush' in source)) {
                    const kind = 'pyth' in source ? 'Pyth pull' : 'Manual'
                    throw new Error(
                        `This mint pair's feed is a ${kind} feed — it can't accept sponsored push updates. Feeds are immutable; use a different mint pair.`,
                    )
                }
                const boundColl = existing.config.collateralFeedId as number[]
                const boundLend = existing.config.lendFeedId as number[]
                if (!bytesEqual(boundColl, collBytes) || !bytesEqual(boundLend, lendBytes)) {
                    throw new Error(
                        "A feed for this mint pair is already bound to a different sponsored account pair. Feeds are immutable — use a different mint pair.",
                    )
                }
            } else {
                const createIx = await feedProgram.methods
                    .create(
                        0,
                        { pythPush: {} },
                        collBytes,
                        lendBytes,
                        DEFAULT_PRICE_TTL_MS,
                        { ...noRules(), maxAgeMs: MAX_PYTH_AGE_MS },
                    )
                    .accounts({
                        feed: feedAccount,
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

            // ── 2. set_from_pyth_push ───────────────────────────────────────────
            const setIx = await feedProgram.methods
                .setFromPythPush()
                .accountsPartial({
                    feed: feedAccount,
                    collateralPriceUpdate: collateralPushAccount,
                    lendPriceUpdate: lendPushAccount,
                })
                .instruction()

            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const setTx = new Transaction({
                blockhash,
                lastValidBlockHeight,
                feePayer: payer,
            }).add(setIx)
            const setSig = await signAndSend(setTx, wallet)
            await connection.confirmTransaction(
                { signature: setSig, blockhash, lastValidBlockHeight },
                'confirmed',
            )

            return { created, signatures: [setSig] }
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

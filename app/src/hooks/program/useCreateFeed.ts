import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FeedRulesInput, noRules } from '../../config/feedRules'
import { feedIdToBytes, MAX_PYTH_AGE_MS } from '../../config/pythFeeds'
import { connection, feedPda, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'

export interface CreateFeedParams {
    collateralMint: PublicKey
    lendMint: PublicKey
    /** Differentiator allowing multiple feeds for the same mint pair. Defaults to 0. */
    id?: number
    /** Pyth feed id (hex) for the collateral side. */
    collateralFeedId: string
    /** Pyth feed id (hex) for the lend side. */
    lendFeedId: string
    /**
     * Optional validation rules baked into the feed at create time. Defaults to
     * the all-disabled sentinel from `noRules()`, matching the on-chain
     * `FeedRules::default()`.
     */
    rules?: FeedRulesInput
}

/**
 * Creates the connected wallet's `feed` account as a Pyth feed declaring the
 * given (collateral, lend) mint pair. The feed PDA is seeded by the authority,
 * so each wallet owns exactly one feed — creation fails if one already exists.
 */
export function useCreateFeed() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            collateralMint,
            lendMint,
            id = 0,
            collateralFeedId,
            lendFeedId,
            rules,
        }: CreateFeedParams): Promise<string> => {
            if (!connected || !wallet?.signTransaction) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)

            const effectiveRules: FeedRulesInput = rules ?? {
                ...noRules(),
                maxAgeMs: MAX_PYTH_AGE_MS,
            }
            const createIx = await feedProgram.methods
                .create(
                    id,
                    { pyth: {} },
                    feedIdToBytes(collateralFeedId),
                    feedIdToBytes(lendFeedId),
                    effectiveRules,
                )
                .accounts({
                    feed: feedPda(collateralMint, lendMint, id),
                    authority: payer,
                    collateralMint,
                    lendMint,
                    payer,
                })
                .instruction()

            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer }).add(
                createIx,
            )
            const signature = await signAndSendV1(tx, wallet)
            await connection.confirmTransaction(
                { signature, blockhash, lastValidBlockHeight },
                'confirmed',
            )
            return signature
        },
        onSuccess: (_sig, { collateralMint, lendMint }) => {
            queryClient.invalidateQueries({
                queryKey: queryKeys.feeds.byPair(
                    collateralMint.toBase58(),
                    lendMint.toBase58(),
                ),
            })
        },
    })
}

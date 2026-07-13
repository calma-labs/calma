import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { type FeedRulesInput, noRules } from '../../config/feedRules'
import { feedIdToBytes, MAX_PYTH_AGE_SECS } from '../../config/pythFeeds'
import { connection, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'

export interface CreateFeedParams {
    collateralMint: PublicKey
    lendMint: PublicKey
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
            collateralFeedId,
            lendFeedId,
            rules,
        }: CreateFeedParams): Promise<string> => {
            if (!connected || !wallet?.signTransaction) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)

            const createIx = await feedProgram.methods
                .create(
                    { pyth: {} },
                    feedIdToBytes(collateralFeedId),
                    feedIdToBytes(lendFeedId),
                    MAX_PYTH_AGE_SECS,
                    rules ?? noRules(),
                )
                .accounts({
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

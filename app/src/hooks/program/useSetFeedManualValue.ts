import * as anchor from '@coral-xyz/anchor'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'

export interface SetFeedManualValueParams {
    /**
     * Manual-source feed to update. The `set_value` account struct derives the
     * PDA from `authority` (the connected wallet) and asserts
     * `feed.config.authority == authority`, so this must match the wallet's
     * own feed.
     */
    feed: PublicKey
    /** Raw normalized prices — see PRICE_SCALE (1e6) in `crates/feed-state`. */
    collateralPrice: anchor.BN
    lendPrice: anchor.BN
    /** Mint pair for cache invalidation, same key the finder uses. */
    collateralMint: PublicKey
    lendMint: PublicKey
}

/**
 * Push new prices onto a Manual-source feed the connected wallet owns.
 * Callers must gate this on `feed.authority == wallet.publicKey`;
 * `set_value` rejects any other signer.
 */
export function useSetFeedManualValue() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            feed,
            collateralPrice,
            lendPrice,
        }: SetFeedManualValueParams): Promise<string> => {
            if (!connected || !wallet?.signTransaction) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)

            const ix = await feedProgram.methods
                .setValue(collateralPrice, lendPrice)
                .accountsPartial({
                    feed,
                    authority: payer,
                })
                .instruction()

            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer }).add(ix)
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

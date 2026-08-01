import { useWalletBalancesStore } from '@/store/wallet.store'
import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { program as readonlyProgram, IRM_PROGRAM_ID, irmStatePda } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'

export interface LeaveParams {
    pool: PublicKey
    lendMint: PublicKey
    /** Number of LP shares to redeem. */
    shares: anchor.BN
}

/**
 * Redeem LP tokens for underlying lend tokens (exit lender position).
 * If the pool has insufficient liquidity the withdrawal is queued on-chain.
 * Automatically invalidates the pool query on success.
 */
export function useLeave() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pool, lendMint, shares }: LeaveParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)

            const tx = await readonlyProgram.methods
                .withdrawLent(shares)
                .accounts({
                    pool,
                    lendMint,
                    authority,
                    rateProgram: IRM_PROGRAM_ID,
                    irmState: irmStatePda(pool),
                })
                .transaction()

            tx.feePayer = authority

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Redeeming LP tokens…', successMessage: 'Redemption submitted!' },
            )
        },
        onSuccess: (_data, { pool }) => {
            const authority = wallet ? new PublicKey(wallet.account.publicKey) : null
            queryClient.invalidateQueries({ queryKey: queryKeys.lending.one(pool) })
            if (authority) {
                queryClient.invalidateQueries({ queryKey: queryKeys.wallet.balances(authority.toBase58()) })
                void useWalletBalancesStore.getState().fetch(authority.toBase58())
            }
        },
    })
}

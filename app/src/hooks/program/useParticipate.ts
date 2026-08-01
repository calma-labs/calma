import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { program as readonlyProgram, IRM_PROGRAM_ID, irmStatePda } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'
import { useWalletBalancesStore } from '../../store/wallet.store'

export interface ParticipateParams {
    pool: PublicKey
    lendMint: PublicKey
    /** The user's source token account holding the lend tokens to deposit. */
    userLendTokenAccount: PublicKey
    /** Raw lend-token amount to deposit (no decimals). */
    amount: anchor.BN
}

/**
 * Deposit lend tokens into a pool (lender side).
 * On success, LP tokens are minted directly to the user's wallet.
 * Automatically invalidates the pool and user-position queries on success.
 */
export function useParticipate() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pool, lendMint, userLendTokenAccount, amount }: ParticipateParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)

            const tx = await readonlyProgram.methods
                .depositLent(amount)
                .accounts({
                    guardProgram: null,
                    guardState: null,
                    pool,
                    lendMint,
                    authority,
                    userLendTokenAccount,
                    rateProgram: IRM_PROGRAM_ID,
                    irmState: irmStatePda(pool),
                })
                .transaction()

            tx.feePayer = authority

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Depositing lend tokens…', successMessage: 'Deposit confirmed!' },
            )
        },
        onSuccess: (_data, { pool }) => {
            const authority = wallet ? new PublicKey(wallet.account.publicKey) : null
            queryClient.invalidateQueries({ queryKey: queryKeys.lending.one(pool) })
            if (authority) {
                queryClient.invalidateQueries({
                    queryKey: queryKeys.userPosition.one(pool, authority),
                })
                void useWalletBalancesStore.getState().fetch(authority.toBase58())
            }
        },
    })
}

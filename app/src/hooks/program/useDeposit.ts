import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'
import { useWalletBalancesStore } from '../../store/wallet.store'
import { useAnchorProgram } from '../useAnchorProgram'

export interface DepositParams {
    pool: PublicKey
    collateralMint: PublicKey
    /** The user's source token account for the collateral. */
    userTokenAccount: PublicKey
    /** Raw token amount (no decimals). */
    amount: anchor.BN
}

/**
 * Deposit collateral tokens into a pool to open or increase a position.
 * Automatically invalidates the pool and user-position queries on success.
 */
export function useDeposit() {
    const { connected, wallet } = useWalletConnection()
    const program = useAnchorProgram()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pool, collateralMint, userTokenAccount, amount }: DepositParams) => {
            if (!connected || !wallet || !program) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)

            console.log('Creating deposit transaction with params:', {
                pool: pool.toBase58(),
                collateralMint: collateralMint.toBase58(),
                userTokenAccount: userTokenAccount.toBase58(),
                amount: amount.toString(),
                authority: authority.toBase58(),
            })

            const tx = await program.methods
                .depositCollateral(amount)
                .accounts({
                    guardProgram: null,
                    guardState: null,
                    pool,
                    collateralMint,
                    authority,
                    userTokenAccount,
                })
                .transaction()

            tx.feePayer = authority

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Depositing collateral…', successMessage: 'Deposit confirmed!' },
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

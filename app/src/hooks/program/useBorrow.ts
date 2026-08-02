import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, IRM_PROGRAM_ID, irmStatePda } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'
import { useWalletBalancesStore } from '../../store/wallet.store'
import { useAnchorProgram } from '../useAnchorProgram'
import { resolveGuardAccounts } from './guardAccounts'
import { refreshFeedForDevnet } from './refreshFeedForDevnet'

export interface BorrowParams {
    pool: PublicKey
    lendMint: PublicKey
    feedState: PublicKey
    /** Raw token amount to borrow (no decimals). */
    amount: anchor.BN
}

/**
 * Borrow lend tokens from a pool against the connected wallet's deposited collateral.
 * Automatically invalidates the pool and user-position queries on success.
 */
export function useBorrow() {
    const { connected, wallet } = useWalletConnection()
    const program = useAnchorProgram()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pool, lendMint, feedState, amount }: BorrowParams) => {
            if (!connected || !wallet || !program) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)

            await refreshFeedForDevnet(connection, pool)
            const guard = await resolveGuardAccounts(connection, pool)

            const tx = await program.methods
                .borrow(amount)
                .accounts({
                    pool,
                    lendMint,
                    authority,
                    rateProgram: IRM_PROGRAM_ID,
                    irmState: irmStatePda(pool),
                    feedState,
                    // Required whenever the market is gated; `null` only when it
                    // is not. See `resolveGuardAccounts`.
                    ...guard,
                })
                .transaction()

            tx.feePayer = authority

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Borrowing…', successMessage: 'Borrow confirmed!' },
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

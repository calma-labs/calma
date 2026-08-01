import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'

export interface ProcessQueueEntryParams {
    pool: PublicKey
    /** The pool's lend mint — queued withdrawals always pay out lend tokens. */
    lendMint: PublicKey
    /** Destination token account for the queued withdrawal (owned by the requester). */
    userTokenAccount: PublicKey
}

/**
 * Process the next collateral-withdrawal queue entry for a pool.
 * Anyone can call this — the payer covers any ATA creation fees.
 */
export function useProcessQueueEntry() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pool, lendMint, userTokenAccount }: ProcessQueueEntryParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)

            const tx = await readonlyProgram.methods
                .processQueueEntry()
                .accounts({
                    pool,
                    lendMint,
                    userTokenAccount,
                })
                .transaction()

            tx.feePayer = payer

            return handleTransaction(
                async () => tx,
                wallet,
                {
                    loadingMessage: 'Processing queue entry…',
                    successMessage: 'Queue entry processed!',
                },
            )
        },
        onSuccess: (_data, { pool }) => {
            queryClient.invalidateQueries({ queryKey: queryKeys.lending.one(pool) })
            queryClient.invalidateQueries({ queryKey: queryKeys.userPosition.byPool(pool) })
        },
    })
}

import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, SystemProgram, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { guardPda, guardProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'

/**
 * Add a pubkey to the connected wallet's guard whitelist.
 *
 * The guard is a per-authority PDA (`["guard", authority]`). If the connected
 * wallet has never created one, we prepend a `create` instruction so the first
 * add also bootstraps the account.
 */
export function useAddToWaitlist() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({ pubkey }: { pubkey: PublicKey }) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)
            const guardState = guardPda(authority)

            const existing = await guardProgram.account.guardState.fetchNullable(guardState)

            const tx = new Transaction()

            if (!existing) {
                const createIx = await guardProgram.methods
                    .create()
                    .accountsPartial({
                        guardState,
                        authority,
                        payer: authority,
                        systemProgram: SystemProgram.programId,
                    })
                    .instruction()
                tx.add(createIx)
            }

            const addIx = await guardProgram.methods
                .add(pubkey)
                .accountsPartial({ guardState, authority })
                .instruction()
            tx.add(addIx)

            tx.feePayer = authority

            return handleTransaction(async () => tx, wallet, {
                loadingMessage: existing
                    ? 'Adding to waitlist…'
                    : 'Creating guard & adding to waitlist…',
                successMessage: 'Added to waitlist!',
            })
        },
        onSuccess: () => {
            if (!wallet) return
            const authority = new PublicKey(wallet.account.publicKey)
            queryClient.invalidateQueries({
                queryKey: queryKeys.guard.state(authority.toBase58()),
            })
        },
    })
}

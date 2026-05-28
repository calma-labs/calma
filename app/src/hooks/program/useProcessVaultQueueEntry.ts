// import { useWalletConnection } from '@solana/react-hooks'
// import { PublicKey } from '@solana/web3.js'
// import { useMutation, useQueryClient } from '@tanstack/react-query'
// import { program as readonlyProgram } from '../../lib/program'
// import { queryKeys } from '../../lib/queryKeys'
// import { handleTransaction } from '../../lib/txHandler'

// export interface ProcessVaultQueueEntryParams {
//     pool: PublicKey
//     lendMint: PublicKey
//     /** The requester whose lend-token withdrawal is being fulfilled. */
//     requester: PublicKey
// }

// /**
//  * Process the next lend-vault withdrawal queue entry for a pool.
//  * The connected wallet acts as payer (covers ATA creation if needed).
//  * Anyone can call this to unblock a queued withdrawal.
//  */
// export function useProcessVaultQueueEntry() {
//     const { connected, wallet } = useWalletConnection()
//     const queryClient = useQueryClient()

//     return useMutation({
//         mutationFn: async ({ pool, lendMint, requester }: ProcessVaultQueueEntryParams) => {
//             if (!connected || !wallet) throw new Error('Wallet not connected')

//             const payer = new PublicKey(wallet.account.publicKey)

//             const tx = await readonlyProgram.methods
//                 .processVaultQueueEntry()
//                 .accounts({
//                     pool,
//                     lendMint,
//                     requester,
//                     payer,
//                 })
//                 .transaction()

//             tx.feePayer = payer

//             return handleTransaction(
//                 async () => tx,
//                 wallet,
//                 {
//                     loadingMessage: 'Processing vault queue entry…',
//                     successMessage: 'Vault queue entry processed!',
//                 },
//             )
//         },
//         onSuccess: (_data, { pool }) => {
//             queryClient.invalidateQueries({ queryKey: queryKeys.lending.one(pool) })
//         },
//     })
// }

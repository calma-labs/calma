import { useWalletConnection } from '@solana/react-hooks'
import {
    PublicKey,
    TransactionMessage,
    VersionedTransaction,
} from '@solana/web3.js'
import { useMutation } from '@tanstack/react-query'
import { connection, guardPda, guardProgram } from '../../lib/program'

export interface SimulateCheckResult {
    /** True when the `check` instruction succeeded (pubkey is whitelisted). */
    whitelisted: boolean
    /** Program logs returned by the simulation, if any. */
    logs: string[]
    /** Friendly error message when the check failed, otherwise null. */
    error: string | null
}

/** Map simulation logs to a friendly message. */
function describeError(logs: string[]): string {
    if (logs.some((l) => l.includes('NotWhitelisted')))
        return 'Pubkey is not in the whitelist'
    if (
        logs.some(
            (l) =>
                l.includes('AccountNotInitialized') ||
                l.includes('could not find account'),
        )
    )
        return 'This authority has no guard account yet'
    return 'Check failed'
}

/**
 * Simulate the guard `check` instruction from the attached wallet.
 *
 * `check` has no side effects — it errors with `NotWhitelisted` when the pubkey
 * is absent and returns `Ok` when present. Running it through
 * `simulateTransaction` (no signature required) tells us the on-chain verdict
 * without spending anything or mutating state.
 */
export function useSimulateCheck() {
    const { connected, wallet } = useWalletConnection()

    return useMutation({
        mutationFn: async ({
            authority,
            pubkey,
        }: {
            authority: PublicKey
            pubkey: PublicKey
        }): Promise<SimulateCheckResult> => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)
            const guardState = guardPda(authority)

            const checkIx = await guardProgram.methods
                .check(pubkey)
                .accountsPartial({ guardState })
                .instruction()

            const { blockhash } = await connection.getLatestBlockhash()
            const message = new TransactionMessage({
                payerKey: payer,
                recentBlockhash: blockhash,
                instructions: [checkIx],
            }).compileToV0Message()
            const tx = new VersionedTransaction(message)

            const sim = await connection.simulateTransaction(tx, {
                sigVerify: false,
                replaceRecentBlockhash: true,
                commitment: 'confirmed',
            })

            const logs = sim.value.logs ?? []
            const whitelisted = sim.value.err === null

            return {
                whitelisted,
                logs,
                error: whitelisted ? null : describeError(logs),
            }
        },
    })
}

import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { guardPda, guardProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'

/** A decoded `GuardState` account paired with its on-chain PDA address. */
export interface GuardStateData {
    /** The `guard_state` PDA. */
    publicKey: PublicKey
    /** Whitelist owner (the account authorised to add/remove entries). */
    authority: PublicKey
    /** Whitelisted pubkeys. */
    whitelist: PublicKey[]
}

async function fetchGuardState(authority: PublicKey): Promise<GuardStateData | null> {
    const publicKey = guardPda(authority)
    const account = await guardProgram.account.guardState.fetchNullable(publicKey)
    if (!account) return null

    return {
        publicKey,
        authority: account.authority,
        whitelist: account.whitelist,
    }
}

/**
 * Fetch the `guard_state` whitelist owned by `authority`.
 *
 * Returns `null` (not an error) when the authority has never created a guard.
 * Disabled until an authority is provided.
 */
export function useGuardState(authority: PublicKey | null) {
    return useQuery({
        queryKey: authority
            ? queryKeys.guard.state(authority.toBase58())
            : ['guard', 'state', 'null'],
        queryFn: () => fetchGuardState(authority!),
        enabled: !!authority,
    })
}

import { type GetProgramAccountsFilter, PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { UserPositionAccount } from '@jbl/wasm-lib'
import { connection, program } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'

export type { UserPositionAccount }

/** Discriminator memcmp filter that selects only UserPosition accounts. */
function discriminatorFilter(): GetProgramAccountsFilter {
    return { memcmp: program.coder.accounts.memcmp('userPosition') }
}

async function fetchUserPosition(address: PublicKey): Promise<UserPositionAccount | null> {
    const info = await connection.getAccountInfo(address)
    if (!info) return null
    return UserPositionAccount.from_bytes(info.data) ?? null
}

async function fetchAllUserPositions(extraFilters: GetProgramAccountsFilter[] = []): Promise<UserPositionAccount[]> {
    const accounts = await connection.getProgramAccounts(program.programId, {
        filters: [discriminatorFilter(), ...extraFilters],
    })
    return accounts.flatMap(({ account }) => {
        const pos = UserPositionAccount.from_bytes(account.data)
        return pos ? [pos] : []
    })
}

/** Derive the UserPosition PDA address for a given pool + authority pair. */
export function getUserPositionAddress(pool: PublicKey, authority: PublicKey): PublicKey {
    const [pda] = PublicKey.findProgramAddressSync(
        [
            Buffer.from('user_position'),
            pool.toBytes(),
            authority.toBytes(),
        ],
        program.programId,
    )
    return pda
}

/** Fetch a single UserPosition by pool + authority. Returns null if the account doesn't exist. */
export function useUserPosition(pool: PublicKey | null, authority: PublicKey | null) {
    return useQuery({
        queryKey: pool && authority ? queryKeys.userPosition.one(pool, authority) : ['user-positions', 'null', 'null'],
        queryFn: async () => {
            const pda = getUserPositionAddress(pool!, authority!)
            return fetchUserPosition(pda)
        },
        enabled: !!pool && !!authority,
    })
}

/** Fetch all UserPosition accounts on-chain. */
export function useUserPositions() {
    return useQuery({
        queryKey: queryKeys.userPosition.all(),
        queryFn: () => fetchAllUserPositions(),
    })
}

/** Fetch all UserPosition accounts for a specific pool. */
export function useUserPositionsByPool(pool: PublicKey | null) {
    return useQuery({
        queryKey: pool ? queryKeys.userPosition.byPool(pool) : ['user-positions', 'pool', 'null'],
        queryFn: () => fetchAllUserPositions([
            { memcmp: { offset: 8 + 32, bytes: pool!.toBase58() } },
        ]),
        enabled: !!pool,
    })
}

/**
 * Fetch all UserPosition accounts owned by a specific authority.
 * Uses a memcmp filter (offset 8 = after discriminator) for on-chain efficiency.
 */
export function useUserPositionsByAuthority(authority: PublicKey | null) {
    return useQuery({
        queryKey: authority ? queryKeys.userPosition.byAuthority(authority) : ['user-positions', 'authority', 'null'],
        queryFn: () => fetchAllUserPositions([
            { memcmp: { offset: 8, bytes: authority!.toBase58() } },
        ]),
        enabled: !!authority,
    })
}

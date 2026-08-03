import { type GetProgramAccountsFilter, PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { PoolAccount, PoolWithIrm } from '@calma/wasm-lib'
import { connection, program } from '../../lib/program'
import { fetchClusterClockTs } from '../../lib/clusterClock'
import { queryKeys } from '../../lib/queryKeys'

export type { PoolAccount }

/** Discriminator memcmp filter that selects only Pool accounts. */
function discriminatorFilter(): GetProgramAccountsFilter {
    return { memcmp: program.coder.accounts.memcmp('pool') }
}

async function fetchPool(address: PublicKey): Promise<PoolWithIrm | null> {
    const info = await connection.getAccountInfo(address)
    if (!info) return null
    const pool = PoolAccount.from_bytes(info.data)
    if (!pool) return null
    const irmInfo = await connection.getAccountInfo(new PublicKey(pool.irm_state))
    if (!irmInfo) return null
    const feedInfo = await connection.getAccountInfo(new PublicKey(pool.feed_state))
    if (!feedInfo) return null
    // Accrual replays are stamped with the cluster clock, not the browser's.
    const clockTs = await fetchClusterClockTs()
    return PoolWithIrm.from_bytes(info.data, irmInfo.data, feedInfo.data, clockTs) ?? null
}

/** Fetch a single pool account by its public key. */
export function useLendingAccount(pool: PublicKey | null) {
    return useQuery({
        queryKey: pool ? queryKeys.lending.one(pool) : ['lending', 'pool', 'null'],
        queryFn: () => fetchPool(pool!),
        enabled: !!pool,
    })
}

export { discriminatorFilter as _poolDiscriminatorFilter, fetchPool as _fetchPool }

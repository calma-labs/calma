import { type GetProgramAccountsFilter, PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { PoolAccount } from '@jbl/wasm-lib'
import { connection, program } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'

export type { PoolAccount }

/** Discriminator memcmp filter that selects only Pool accounts. */
function discriminatorFilter(): GetProgramAccountsFilter {
    return { memcmp: program.coder.accounts.memcmp('pool') }
}

async function fetchPool(address: PublicKey): Promise<PoolAccount | null> {
    const info = await connection.getAccountInfo(address)
    if (!info) return null
    return PoolAccount.from_bytes(info.data) ?? null
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

import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { PoolAccount, PoolWithIrm } from '@calma/wasm-lib'
import { connection, program } from '../../lib/program'
import { fetchClusterClockTs } from '../../lib/clusterClock'
import { queryKeys } from '../../lib/queryKeys'
import { _poolDiscriminatorFilter } from './useLendingAccount'

export type { PoolAccount }

export interface PoolAccountWithKey {
    publicKey: PublicKey
    account: PoolWithIrm
}

async function fetchAllPools(): Promise<PoolAccountWithKey[]> {
    const accounts = await connection.getProgramAccounts(program.programId, {
        filters: [_poolDiscriminatorFilter()],
    })

    const parsed = accounts.flatMap(({ pubkey, account }) => {
        const pool = PoolAccount.from_bytes(account.data)
        if (!pool) return []
        const rateStatePubkey = new PublicKey(pool.irm_state)
        const feedStatePubkey = new PublicKey(pool.feed_state)
        return [{ pubkey, raw: account.data, rateStatePubkey, feedStatePubkey }]
    })

    if (parsed.length === 0) return []

    const [irmInfos, feedInfos, clockTs] = await Promise.all([
        connection.getMultipleAccountsInfo(parsed.map((p) => p.rateStatePubkey)),
        connection.getMultipleAccountsInfo(parsed.map((p) => p.feedStatePubkey)),
        // Accrual replays are stamped with the cluster clock, not the browser's.
        fetchClusterClockTs(),
    ])

    return parsed.flatMap(({ pubkey, raw }, i) => {
        const irmInfo = irmInfos[i]
        const feedInfo = feedInfos[i]
        if (!irmInfo || !feedInfo) return []

        const poolWithIrm = PoolWithIrm.from_bytes(raw, irmInfo.data, feedInfo.data, clockTs)
        return poolWithIrm ? [{ publicKey: pubkey, account: poolWithIrm }] : []
    })
}

export function useLendingAccounts() {
    return useQuery({
        queryKey: queryKeys.lending.all(),
        queryFn: fetchAllPools,
    })
}

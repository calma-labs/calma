import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { PoolAccount, PoolWithIrm } from '@jbl/wasm-lib'
import { connection, feedPda, feedProgram, program } from '../../lib/program'
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

    const irmInfos = await connection.getMultipleAccountsInfo(
        parsed.map((p) => p.rateStatePubkey),
    )
    // TODO: Fetch concurrently
    const feedInfos = await connection.getMultipleAccountsInfo(
        parsed.map((p) => p.feedStatePubkey),
    )


    return parsed.flatMap(({ pubkey, raw, feedStatePubkey }, i) => {
        const irmInfo = irmInfos[i]
        const feedInfo = feedInfos[i]
        if (!irmInfo || !feedInfo) return []

        try {
            const decoded = feedProgram.coder.accounts.decode('feed', feedInfo.data) as {
                config: { authority: PublicKey }
                data: { collateralMint: PublicKey; lendMint: PublicKey }
            }
            const expectedPda = feedPda(
                new PublicKey(decoded.config.authority),
                new PublicKey(decoded.data.collateralMint),
                new PublicKey(decoded.data.lendMint),
            )
            if (!expectedPda.equals(feedStatePubkey)) return []
        } catch {
            return []
        }

        const poolWithIrm = PoolWithIrm.from_bytes(raw, irmInfo.data, feedInfo.data)
        return poolWithIrm ? [{ publicKey: pubkey, account: poolWithIrm }] : []
    })
}

export function useLendingAccounts() {
    return useQuery({
        queryKey: queryKeys.lending.all(),
        queryFn: fetchAllPools,
    })
}

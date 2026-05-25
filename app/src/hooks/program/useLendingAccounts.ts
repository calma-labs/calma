import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { PoolAccount } from '@jbl/wasm-lib'
import { connection, program } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { _poolDiscriminatorFilter } from './useLendingAccount'

export type { PoolAccount }

export interface PoolAccountWithKey {
    publicKey: PublicKey
    account: PoolAccount
}

async function fetchAllPools(): Promise<PoolAccountWithKey[]> {
    const accounts = await connection.getProgramAccounts(program.programId, {
        filters: [_poolDiscriminatorFilter()],
    })
    return accounts.flatMap(({ pubkey, account }) => {
        const pool = PoolAccount.from_bytes(account.data)
        return pool ? [{ publicKey: pubkey, account: pool }] : []
    })
}

export function useLendingAccounts() {
    return useQuery({
        queryKey: queryKeys.lending.all(),
        queryFn: fetchAllPools,
    })
}

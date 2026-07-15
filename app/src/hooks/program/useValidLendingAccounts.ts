import { isTokenValid, POOL_BLACKLIST } from '@/lib/validation'
import { getTokenMeta } from '@/lib/tokenRegistry'
import { PublicKey } from '@solana/web3.js'
import { useEffect, useState } from 'react'
import { type PoolAccountWithKey, useLendingAccounts } from './useLendingAccounts'

export type { PoolAccountWithKey }

async function poolHasValidMinter(pool: PoolAccountWithKey): Promise<boolean> {
    if (POOL_BLACKLIST.has(pool.publicKey.toBase58())) return false
    const collateralMint = new PublicKey(pool.account.collateral_mint)
    const lendMint = new PublicKey(pool.account.lend_mint)
    if (!getTokenMeta(collateralMint.toBase58())) return false
    if (!getTokenMeta(lendMint.toBase58())) return false
    const [collateralValid, lendValid] = await Promise.all([
        isTokenValid(collateralMint),
        isTokenValid(lendMint),
    ])
    return collateralValid || lendValid
}

/**
 * Returns only pools whose collateral or lend mint has the valid faucet authority.
 * Mirrors the filtering used in useMultiplyStrategies.
 */
export function useValidLendingAccounts() {
    const { data: poolsData, isLoading, error } = useLendingAccounts()
    const [validPools, setValidPools] = useState<PoolAccountWithKey[]>([])
    const [isValidating, setIsValidating] = useState(false)

    useEffect(() => {
        if (!poolsData || poolsData.length === 0) {
            setValidPools([])
            return
        }
        setIsValidating(true)
        Promise.all(
            poolsData.map(async (pd) => {
                const isValid = await poolHasValidMinter(pd)
                return { pd, isValid }
            }),
        )
            .then((results) => {
                setValidPools(results.filter((r) => r.isValid).map((r) => r.pd))
                setIsValidating(false)
            })
            .catch(() => {
                setValidPools([])
                setIsValidating(false)
            })
    }, [poolsData])

    return { data: validPools, isLoading: isLoading || isValidating, error: error };
}

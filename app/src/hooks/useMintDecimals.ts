import { getMint } from '@solana/spl-token'
import { PublicKey } from '@solana/web3.js'
import { useQuery, useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
import { connection } from '../lib/program'

/**
 * Fetches the decimal places of a SPL/SPL-2022 mint.
 * Result is cached indefinitely — mint decimals are immutable.
 */
export function useMintDecimals(mint: PublicKey | null) {
    return useQuery({
        queryKey: ['mint', 'decimals', mint?.toBase58() ?? 'null'],
        queryFn: async () => {
            const info = await getMint(connection, mint!)
            return info.decimals
        },
        enabled: !!mint,
        staleTime: Infinity,
        gcTime: Infinity,
    })
}

/**
 * Fetches decimals for multiple mints in parallel.
 * Returns a Map<base58, decimals> populated as results arrive.
 */
export function useMintDecimalsMap(mints: PublicKey[]): Map<string, number> {
    const results = useQueries({
        queries: mints.map((mint) => ({
            queryKey: ['mint', 'decimals', mint.toBase58()],
            queryFn: async () => {
                const info = await getMint(connection, mint)
                return info.decimals
            },
            staleTime: Infinity,
            gcTime: Infinity,
        })),
    })

    return useMemo(() => {
        const map = new Map<string, number>()
        results.forEach((result, i) => {
            if (result.data != null) map.set(mints[i].toBase58(), result.data)
        })
        return map
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [results])
}

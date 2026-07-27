import { HermesClient } from '@pythnetwork/hermes-client'
import { useQuery } from '@tanstack/react-query'
import { hermesId } from '../config/pythFeeds'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

/** Raw integer values from a single Hermes price feed. */
export interface PythRawData {
    /** Raw i64 mantissa from Pyth (must be multiplied by 10^expo to get USD). */
    price: number
    expo: number
    /** Raw u64 confidence interval (same exponent as price). */
    conf: number
    publishTime: number
    /** Raw i64 EMA price (same exponent as price). */
    emaPrice: number
}

/**
 * Live raw integer Pyth data for a single feed id, polled from Hermes every 5 s.
 * Returns the mantissa, exponent, confidence, and EMA price — the values needed
 * to replay the on-chain `FeedRules` checks exactly.
 * Pass `id` = null to disable.
 */
export function usePythRawData(id: string | null) {
    return useQuery({
        queryKey: ['pyth-raw', id],
        enabled: !!id,
        refetchInterval: 5_000,
        queryFn: async (): Promise<PythRawData> => {
            const res = await hermes.getLatestPriceUpdates([hermesId(id!)], {
                parsed: true,
            })
            const parsed = res.parsed?.[0]
            if (!parsed) throw new Error('No price returned from Hermes')
            return {
                price: Number(parsed.price.price),
                expo: parsed.price.expo,
                conf: Number(parsed.price.conf),
                publishTime: parsed.price.publish_time,
                emaPrice: Number(parsed.ema_price.price),
            }
        },
    })
}

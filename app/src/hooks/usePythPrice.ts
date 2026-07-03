import { HermesClient } from '@pythnetwork/hermes-client'
import { useQuery } from '@tanstack/react-query'
import { hermesId } from '../config/pythFeeds'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

export interface PythLivePrice {
    /** Human-readable price in USD, already exponent-adjusted. */
    price: number
    /** Confidence interval in USD. */
    confidence: number
    /** Publish time (unix seconds). */
    publishTime: number
}

/** Parse a Hermes parsed-price entry (`price` is an integer string, `expo` ≤ 0). */
function toLivePrice(price: string, expo: number, conf: string, publishTime: number): PythLivePrice {
    const scale = 10 ** expo
    return {
        price: Number(price) * scale,
        confidence: Number(conf) * scale,
        publishTime,
    }
}

/**
 * Live price for a single Pyth feed id, polled from Hermes every 5s.
 * Pass `id` = null to disable (e.g. no feed selected).
 */
export function usePythPrice(id: string | null) {
    return useQuery({
        queryKey: ['pyth-price', id],
        enabled: !!id,
        refetchInterval: 5_000,
        queryFn: async (): Promise<PythLivePrice> => {
            const res = await hermes.getLatestPriceUpdates([hermesId(id!)], {
                parsed: true,
            })
            const parsed = res.parsed?.[0]
            if (!parsed) throw new Error('No price returned from Hermes')
            const { price, expo, conf, publish_time } = parsed.price
            return toLivePrice(price, expo, conf, publish_time)
        },
    })
}

import { HermesClient } from '@pythnetwork/hermes-client'
import { useQuery } from '@tanstack/react-query'
import { pythFeedFromMetadata, type PythFeed } from '../config/pythFeeds'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

/**
 * Look up the Pyth price feed(s) for a token by querying Hermes' catalog.
 *
 * Enforces the 1-to-1 token ↔ feed relation the Feed page needs: pass a token's
 * asset ticker (see `pythQueryForToken`) and get back the matching USD-quoted
 * feeds. Usually that's a single feed; when the catalog returns several the UI
 * lets the user pick the exact one. Pass `query` = null to disable.
 */
export function usePythFeeds(query: string | null) {
    return useQuery({
        queryKey: ['pyth-feeds', query],
        enabled: !!query,
        staleTime: 5 * 60_000,
        queryFn: async (): Promise<PythFeed[]> => {
            const metas = await hermes.getPriceFeeds({ query: query! })
            return metas
                .filter(
                    (m) => (m.attributes.quote_currency ?? '').toUpperCase() === 'USD',
                )
                .map(pythFeedFromMetadata)
        },
    })
}

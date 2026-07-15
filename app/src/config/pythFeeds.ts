import type { PriceFeedMetadata } from '@pythnetwork/hermes-client'
import { PublicKey } from '@solana/web3.js'

/**
 * Catalog of Pyth price feeds selectable on the Feed page.
 *
 * `id` is the Pyth price-feed id (hex, no `0x`). These ids are identical across
 * Hermes / all Solana clusters — see https://pyth.network/developers/price-feed-ids.
 * The Feed page fetches the live price from Hermes and posts a signed price
 * update to the on-chain `feed` program via the Pyth Pull flow.
 */
export interface PythFeed {
    /** Pyth price-feed id, hex without the `0x` prefix. */
    id: string
    symbol: string
    name: string
    icon: string
}

const ICON = (sym: string) =>
    `https://raw.githubusercontent.com/spothq/cryptocurrency-icons/master/128/color/${sym.toLowerCase()}.png`

/**
 * The lend side of every feed is pinned to USDC/USD (≈ $1), so the stored
 * ratio (`collateral_price / lend_price`) tracks the asset's USD price.
 */
export const USDC_USD_FEED_ID =
    'eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a'

/**
 * Placeholder mint used for both sides when the Feed page has to `create` a
 * scratch feed under the connected wallet. Wrapped SOL exists on every cluster,
 * so `create` (which requires `Account<Mint>` inputs) always succeeds. The feed
 * mints only affect the decimal-adjusted ratio returned by `get_state` — the
 * raw Pyth prices this page pushes are independent of them — so a scratch
 * inspection feed doesn't need real pool mints.
 */
export const PLACEHOLDER_MINT = new PublicKey(
    'So11111111111111111111111111111111111111112',
)

/** Freshness window (milliseconds) stored in `rules.max_age_ms` at feed create time. */
export const MAX_PYTH_AGE_MS = 60_000

/**
 * Derive a Pyth catalog search query from a whitelisted token symbol.
 *
 * The Feed page fetches a token's feed live from Pyth's `getPriceFeeds` catalog
 * (which matches on the feed's `symbol` substring), so the query has to be the
 * token's underlying asset ticker. xStock tokens carry a trailing lowercase `x`
 * (e.g. `TSLAx`) that Pyth's equity symbols (`Equity.US.TSLA/USD`) don't have,
 * so strip it. Everything else queries by its symbol as-is.
 */
export function pythQueryForToken(symbol: string): string {
    return /^[A-Z]{2,}x$/.test(symbol) ? symbol.slice(0, -1) : symbol
}

/** Convert a Hermes `getPriceFeeds` catalog entry into a `PythFeed`. */
export function pythFeedFromMetadata(meta: PriceFeedMetadata): PythFeed {
    const attrs = meta.attributes
    const base = attrs.base ?? attrs.generic_symbol ?? meta.id
    const quote = attrs.quote_currency ?? 'USD'
    return {
        id: meta.id.startsWith('0x') ? meta.id.slice(2) : meta.id,
        symbol: base,
        name: attrs.display_symbol ?? `${base}/${quote}`,
        icon: ICON(base),
    }
}

/** `0x`-prefixed form Hermes expects for its `ids` query param. */
export function hermesId(id: string): string {
    return id.startsWith('0x') ? id : `0x${id}`
}

/** Convert a hex feed id into the 32-byte array the `feed` program expects. */
export function feedIdToBytes(id: string): number[] {
    const hex = id.startsWith('0x') ? id.slice(2) : id
    if (hex.length !== 64) throw new Error(`Invalid Pyth feed id: ${id}`)
    const bytes: number[] = []
    for (let i = 0; i < 64; i += 2) bytes.push(parseInt(hex.slice(i, i + 2), 16))
    return bytes
}

/**
 * Encode a 32-byte feed id (as stored on-chain) back into the un-prefixed hex
 * form the rest of the app + Hermes uses.
 */
export function bytesToFeedIdHex(bytes: number[]): string {
    if (bytes.length !== 32) throw new Error(`Invalid feed id byte length: ${bytes.length}`)
    return bytes.map((b) => b.toString(16).padStart(2, '0')).join('')
}

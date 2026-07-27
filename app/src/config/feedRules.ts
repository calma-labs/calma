import * as anchor from '@coral-xyz/anchor'

/**
 * Shape accepted by the `feed::create` Anchor method for its `rules` arg. Field
 * names match the Anchor-generated camelCased IDL: `rules._reserved` becomes
 * `reserved` (leading underscore stripped).
 */
export interface FeedRulesInput {
    /** Reject if `conf / price` exceeds this many bps. `0` disables. */
    maxConfBps: number
    /** Reject if `|new − last| / last` exceeds `bps_per_hour × elapsed_hours`. `0` disables. */
    maxDeviationBpsPerHour: number
    /** Reject if `|price − ema_price| / ema_price` exceeds this many bps. `0` disables. */
    emaDivergenceBps: number
    /** Reject if normalized price is below this floor (PRICE_SCALE units). `0` disables. */
    minPrice: anchor.BN
    /** Reject if normalized price is above this ceiling (PRICE_SCALE units). `0` disables. */
    maxPrice: anchor.BN
    /** Reject if Pyth price age exceeds this many milliseconds. Required (> 0) for Pyth feeds; `0` disables. */
    maxAgeMs: number
    /** Trailing padding. */
    reserved: number[]
}

/**
 * All-disabled rules struct — matches the on-chain `FeedRules::default()`.
 * Every field's `0` means "rule inactive"; use this when the caller does not
 * want any Pyth-side validation beyond the built-in staleness check.
 */
export function noRules(): FeedRulesInput {
    return {
        maxConfBps: 0,
        maxDeviationBpsPerHour: 0,
        emaDivergenceBps: 0,
        minPrice: new anchor.BN(0),
        maxPrice: new anchor.BN(0),
        maxAgeMs: 0,
        reserved: Array<number>(4).fill(0),
    }
}

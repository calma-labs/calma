//! Optional Pyth-side validation gates configured per-feed via [`FeedRules`].
//!
//! Every rule uses `0` as the "disabled" sentinel so a zero-init `FeedRules`
//! reproduces the pre-rules behavior. All arithmetic is done in `u128` to keep
//! the checks total; comparisons that would overflow are treated as rule
//! violations, not silent passes.

use crate::error::ErrorCode;
use crate::state::FeedRules;
use anchor_lang::prelude::*;

const BPS_DENOM: u128 = 10_000;

/// `conf / price ≤ max_conf_bps` (basis points). `price` is the raw signed
/// Pyth value; a non-positive price is treated as a violation because it
/// would have failed `normalize()` anyway and we don't want to divide by zero.
pub fn check_conf(price: i64, conf: u64, max_conf_bps: u16) -> Result<()> {
    if max_conf_bps == 0 {
        return Ok(());
    }
    require!(price > 0, ErrorCode::ConfidenceTooWide);
    let price = price as u128;
    let conf = conf as u128;
    let lhs = conf
        .checked_mul(BPS_DENOM)
        .ok_or(ErrorCode::ConfidenceTooWide)?;
    let rhs = price
        .checked_mul(max_conf_bps as u128)
        .ok_or(ErrorCode::ConfidenceTooWide)?;
    require!(lhs <= rhs, ErrorCode::ConfidenceTooWide);
    Ok(())
}

/// `min_price ≤ normalized ≤ max_price`. Either bound at `0` is disabled.
pub fn check_bounds(normalized: u64, min_price: u64, max_price: u64) -> Result<()> {
    if min_price > 0 {
        require!(normalized >= min_price, ErrorCode::PriceOutOfBounds);
    }
    if max_price > 0 {
        require!(normalized <= max_price, ErrorCode::PriceOutOfBounds);
    }
    Ok(())
}

/// `|price − ema_price| / ema_price ≤ ema_divergence_bps`. Skipped when the
/// EMA is missing (`ema_price <= 0`) so a feed publisher that hasn't warmed
/// up its EMA doesn't lock the pool out.
pub fn check_ema_divergence(price: i64, ema_price: i64, ema_divergence_bps: u16) -> Result<()> {
    if ema_divergence_bps == 0 || ema_price <= 0 {
        return Ok(());
    }
    require!(price > 0, ErrorCode::EmaDivergenceTooLarge);
    let price = price as u128;
    let ema = ema_price as u128;
    let diff = if price >= ema { price - ema } else { ema - price };
    let lhs = diff
        .checked_mul(BPS_DENOM)
        .ok_or(ErrorCode::EmaDivergenceTooLarge)?;
    let rhs = ema
        .checked_mul(ema_divergence_bps as u128)
        .ok_or(ErrorCode::EmaDivergenceTooLarge)?;
    require!(lhs <= rhs, ErrorCode::EmaDivergenceTooLarge);
    Ok(())
}

/// Time-scaled deviation guard. The allowed jump grows with the elapsed time
/// since the last accepted price so feeds that are updated infrequently
/// (hours or days apart) aren't rejected on legitimate price moves.
///
/// The budget is **purely** time-scaled, with no ceiling. It used to be clamped
/// at 10_000 bps (100%), which silently made any move larger than a doubling
/// unrepresentable: `diff / last > 100%` failed the comparison no matter how
/// much time had passed, so a feed whose asset genuinely tripled would reject
/// every subsequent update and stay frozen at the stale price forever — the
/// worst possible failure for a price oracle. Staleness is bounded by
/// `max_age_ms`, not by this gate; this one only rates how fast a price may
/// move while it *is* being updated.
///
/// Skipped on the first update (`last == 0` or `last_ts == 0`).
pub fn check_deviation(
    new: u64,
    last: u64,
    last_ts: i64,
    now_ts: i64,
    max_deviation_bps_per_hour: u16,
) -> Result<()> {
    if max_deviation_bps_per_hour == 0 || last == 0 || last_ts == 0 {
        return Ok(());
    }
    let elapsed_secs: i64 = now_ts.saturating_sub(last_ts).max(0);
    // Ceiling division: any positive elapsed time gives at least one hour of
    // budget so a legitimate update seconds after the previous one isn't
    // gated purely by rounding.
    let elapsed_hours = ((elapsed_secs as u128) + 3_599) / 3_600;
    let effective_bps = elapsed_hours.saturating_mul(max_deviation_bps_per_hour as u128);

    let new = new as u128;
    let last = last as u128;
    let diff = if new >= last { new - last } else { last - new };
    // Saturating rather than checked: an overflowing budget means "so much time
    // has passed that any move is allowed", which must accept, not reject.
    let lhs = diff.saturating_mul(BPS_DENOM);
    let rhs = last.saturating_mul(effective_bps);
    require!(lhs <= rhs, ErrorCode::PriceDeviationTooLarge);
    Ok(())
}

/// Reject if the price is older than `max_age_ms` milliseconds.
/// `max_age_ms == 0` disables the check (returns `Ok`), matching every other
/// rule in this module.
///
/// **Not the same convention as `interface::price_stale_at`**, which takes a
/// similarly named budget and treats `0` as *reject everything*. The difference
/// is deliberate and the two are not interchangeable: this gate is an opt-in
/// validation on *ingestion*, so `0` means "don't run it"; that one gates
/// *consumption*, so `0` means "no budget, refuse". Disabling this one is safe
/// precisely because the consumption gate still applies — `set_from_pyth` stamps
/// `last_updated_ts` from the Pyth publish time, so an old price ingested here
/// still reads as old to every consumer.
///
/// The comparison is done entirely in milliseconds so a configured age is never
/// truncated on its way to the gate. Note the *measurement* is still bounded by
/// its inputs: both `publish_time` and `clock_ts` are Unix seconds on Solana, so
/// elapsed time only ever lands on whole-second multiples. Millisecond
/// configuration therefore buys precision in the threshold, not in the clock —
/// `max_age_ms = 1_500` admits a one-second-old price and rejects a two-second
/// one, where the previous `max_age_ms / 1_000` truncation silently reduced it
/// to 1_000 and a sub-second setting collapsed to 0, rejecting everything.
///
/// A `publish_time` ahead of the clock (Pyth can publish slightly early) yields
/// a negative elapsed time and passes, which is correct — it is not stale.
pub fn check_max_age(publish_time: i64, clock_ts: i64, max_age_ms: u32) -> Result<()> {
    if max_age_ms == 0 {
        return Ok(());
    }
    let elapsed_ms = (clock_ts as i128 - publish_time as i128) * 1_000;
    require!(
        elapsed_ms <= max_age_ms as i128,
        ErrorCode::StalePushPrice
    );
    Ok(())
}

/// Reject an update whose price is older than the one already written.
///
/// `set_from_pyth` / `set_from_pyth_push` are permissionless by design — anyone
/// may relay a signed Pyth update, which is what keeps the feed live. But
/// nothing about a signature says *when* it was chosen: Wormhole VAAs stay
/// verifiable forever, so a caller can post whichever update inside the
/// `max_age_ms` ingestion window suits them rather than the most recent one.
/// Without this gate the written timestamp could also move *backwards*, which
/// both re-opens an already-superseded price and widens the deviation budget
/// (`check_deviation` scales it by `now − last_ts`).
///
/// The comparison is `>=`, not `>`. The feed stamps `last_updated_ts` from the
/// **min** of the two legs' publish times, so a strict `>` would refuse an
/// update in which one leg advanced and the other had not yet republished —
/// turning a routine cadence mismatch into a stuck feed. Allowing equality
/// costs nothing: a re-post at the same publish time carries the same price for
/// the same feed id.
///
/// This bounds *selection*, not staleness — `check_max_age` still caps how old
/// an accepted update may be, and consumers still apply `price_ttl_ms`.
pub fn check_monotonic(new_ts: i64, last_ts: i64) -> Result<()> {
    require!(new_ts >= last_ts, ErrorCode::NonMonotonicPrice);
    Ok(())
}

/// Cross-field validation applied at `create` time (Pyth source only).
///
/// `create` is the only write site for `FeedRules` — there is no setter — so
/// whatever passes here is what the feed lives with.
pub fn validate_rules(rules: &FeedRules) -> Result<()> {
    if rules.min_price > 0 && rules.max_price > 0 {
        require!(rules.min_price <= rules.max_price, ErrorCode::InvalidRules);
    }
    // A deviation budget tight enough to reject ordinary volatility does not
    // protect the market, it freezes it: the update is refused, the price goes
    // stale, and every borrow and collateral withdrawal fails until enough time
    // accrues. Disabling the guard (`0`) is a legitimate choice; setting it to
    // something a real price move cannot clear is not one anybody makes on
    // purpose. See `MIN_DEVIATION_BPS_PER_HOUR`.
    if rules.max_deviation_bps_per_hour > 0 {
        require!(
            rules.max_deviation_bps_per_hour >= crate::state::MIN_DEVIATION_BPS_PER_HOUR,
            ErrorCode::InvalidRules
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_disabled_when_zero_bps() {
        assert!(check_conf(1_000_000, u64::MAX, 0).is_ok());
    }

    #[test]
    fn conf_at_limit_accepts() {
        // 100 conf on 1_000_000 price = 1 bps.
        assert!(check_conf(1_000_000, 100, 1).is_ok());
    }

    #[test]
    fn conf_over_limit_rejects() {
        // 101 conf on 1_000_000 price > 1 bps.
        assert!(check_conf(1_000_000, 101, 1).is_err());
    }

    #[test]
    fn bounds_disabled_when_zero() {
        assert!(check_bounds(1, 0, 0).is_ok());
        assert!(check_bounds(u64::MAX, 0, 0).is_ok());
    }

    #[test]
    fn bounds_reject_out_of_range() {
        assert!(check_bounds(9, 10, 100).is_err());
        assert!(check_bounds(101, 10, 100).is_err());
        assert!(check_bounds(50, 10, 100).is_ok());
    }

    #[test]
    fn ema_divergence_disabled_or_missing_ema() {
        assert!(check_ema_divergence(100, 200, 0).is_ok());
        assert!(check_ema_divergence(100, 0, 500).is_ok());
        assert!(check_ema_divergence(100, -1, 500).is_ok());
    }

    #[test]
    fn ema_divergence_within_and_outside() {
        // 100 vs 100 = 0 divergence.
        assert!(check_ema_divergence(100, 100, 500).is_ok());
        // 105 vs 100 = 500 bps exactly.
        assert!(check_ema_divergence(105, 100, 500).is_ok());
        // 106 vs 100 = 600 bps > 500.
        assert!(check_ema_divergence(106, 100, 500).is_err());
    }

    #[test]
    fn deviation_skipped_on_first_update() {
        assert!(check_deviation(1_000_000, 0, 0, 1_000, 100).is_ok());
    }

    #[test]
    fn deviation_budget_scales_with_elapsed_time() {
        // 100 bps/hour, elapsed = 1 hour, so 10% move is allowed.
        let last: u64 = 1_000_000;
        let new_within: u64 = 1_010_000; // +100 bps
        let new_over: u64 = 1_020_000; // +200 bps
        assert!(check_deviation(new_within, last, 1_000, 1_000 + 3_600, 100).is_ok());
        assert!(check_deviation(new_over, last, 1_000, 1_000 + 3_600, 100).is_err());
        // Same +200 bps move accepted over 2 hours.
        assert!(check_deviation(new_over, last, 1_000, 1_000 + 7_200, 100).is_ok());
    }

    #[test]
    fn max_age_disabled_when_zero() {
        assert!(check_max_age(1_700_000_000, 1_700_001_000, 0).is_ok());
    }

    #[test]
    fn max_age_within_limit_accepts() {
        // 30s elapsed × 1000 = 30_000 ms ≤ 60_000 ms limit.
        assert!(check_max_age(1_700_000_000, 1_700_000_030, 60_000).is_ok());
    }

    #[test]
    fn max_age_at_limit_accepts() {
        // Exactly 60s elapsed = 60_000 ms = limit.
        assert!(check_max_age(1_700_000_000, 1_700_000_060, 60_000).is_ok());
    }

    #[test]
    fn max_age_over_limit_rejects() {
        // 61s elapsed = 61_000 ms > 60_000 ms limit.
        assert!(check_max_age(1_700_000_000, 1_700_000_061, 60_000).is_err());
    }

    #[test]
    fn max_age_sub_second_threshold_is_not_truncated() {
        // 1_500 ms admits a 1s-old price and rejects a 2s-old one. Under the old
        // `max_age_ms / 1_000` truncation this behaved as 1_000 ms.
        assert!(check_max_age(1_700_000_000, 1_700_000_001, 1_500).is_ok());
        assert!(check_max_age(1_700_000_000, 1_700_000_002, 1_500).is_err());
        // A sub-second budget admits only a same-second price — it no longer
        // collapses to "reject everything".
        assert!(check_max_age(1_700_000_000, 1_700_000_000, 500).is_ok());
        assert!(check_max_age(1_700_000_000, 1_700_000_001, 500).is_err());
    }

    #[test]
    fn max_age_accepts_a_price_published_ahead_of_the_clock() {
        assert!(check_max_age(1_700_000_005, 1_700_000_000, 1_000).is_ok());
    }

    #[test]
    fn rules_reject_a_deviation_budget_too_tight_to_track_a_real_move() {
        let tight = FeedRules {
            max_deviation_bps_per_hour: crate::state::MIN_DEVIATION_BPS_PER_HOUR - 1,
            ..Default::default()
        };
        assert!(validate_rules(&tight).is_err());

        let at_floor = FeedRules {
            max_deviation_bps_per_hour: crate::state::MIN_DEVIATION_BPS_PER_HOUR,
            ..Default::default()
        };
        assert!(validate_rules(&at_floor).is_ok());

        // `0` still means "disabled" — an explicit opt-out, not a tight budget.
        let disabled = FeedRules {
            max_deviation_bps_per_hour: 0,
            ..Default::default()
        };
        assert!(validate_rules(&disabled).is_ok());
    }

    #[test]
    fn monotonic_rejects_only_backwards_updates() {
        // Forward and equal are both fine; equal is what keeps a feed whose two
        // legs republish at different cadences from getting stuck.
        assert!(check_monotonic(1_700_000_100, 1_700_000_000).is_ok());
        assert!(check_monotonic(1_700_000_000, 1_700_000_000).is_ok());
        // A replayed older VAA — the case the gate exists for.
        assert!(check_monotonic(1_699_999_999, 1_700_000_000).is_err());
    }

    #[test]
    fn monotonic_accepts_the_first_ever_update() {
        // A fresh feed carries last_updated_ts == 0, so nothing is rejected
        // before the first price lands.
        assert!(check_monotonic(1_700_000_000, 0).is_ok());
    }

    #[test]
    fn max_age_extreme_timestamps_do_not_overflow() {
        assert!(check_max_age(i64::MIN, i64::MAX, 1_000).is_err());
        assert!(check_max_age(i64::MAX, i64::MIN, 1_000).is_ok());
    }

    #[test]
    fn deviation_budget_is_uncapped_so_large_moves_eventually_pass() {
        // With 100 bps/hour, 100 hours buys a 100% budget: a doubling passes, a
        // tripling does not — yet.
        let last: u64 = 1_000_000;
        assert!(check_deviation(2_000_000, last, 1_000, 1_000 + 100 * 3_600, 100).is_ok());
        assert!(check_deviation(3_000_000, last, 1_000, 1_000 + 100 * 3_600, 100).is_err());
        // Given 200 hours the budget reaches 200% and the tripling is accepted.
        // Under the old 100% clamp this stayed rejected forever, freezing the
        // feed at a stale price.
        assert!(check_deviation(3_000_000, last, 1_000, 1_000 + 200 * 3_600, 100).is_ok());
        // A 10x move clears once enough time has accrued.
        assert!(check_deviation(10_000_000, last, 1_000, 1_000 + 900 * 3_600, 100).is_ok());
    }

    #[test]
    fn deviation_extreme_inputs_stay_total() {
        // A 100% move with an enormous time budget is accepted.
        assert!(check_deviation(u64::MAX, u64::MAX / 2, 1, i64::MAX, u16::MAX).is_ok());
        // The guard is *relative*, so an astronomically large ratio is still
        // refused however long has elapsed — from a price of 1, reaching
        // u64::MAX is a ~1.8e19x move and no realistic budget covers it. The
        // point here is that it decides without panicking.
        assert!(check_deviation(u64::MAX, 1, 1, i64::MAX, u16::MAX).is_err());
    }
}

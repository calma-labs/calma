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
/// (hours or days apart) aren't rejected on legitimate price moves. The
/// effective ceiling is clamped at 10_000 bps (100%), which for any tightly
/// configured feed produces an eventual "any move accepted" horizon.
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
    let raw_budget = elapsed_hours.saturating_mul(max_deviation_bps_per_hour as u128);
    let effective_bps = raw_budget.min(BPS_DENOM);

    let new = new as u128;
    let last = last as u128;
    let diff = if new >= last { new - last } else { last - new };
    let lhs = diff
        .checked_mul(BPS_DENOM)
        .ok_or(ErrorCode::PriceDeviationTooLarge)?;
    let rhs = last
        .checked_mul(effective_bps)
        .ok_or(ErrorCode::PriceDeviationTooLarge)?;
    require!(lhs <= rhs, ErrorCode::PriceDeviationTooLarge);
    Ok(())
}

/// Reject if `clock_ts − publish_time` (in seconds) exceeds `max_age_ms`
/// milliseconds. `max_age_ms == 0` disables the check (returns `Ok`).
pub fn check_max_age(publish_time: i64, clock_ts: i64, max_age_ms: u32) -> Result<()> {
    if max_age_ms == 0 {
        return Ok(());
    }
    let elapsed_ms = clock_ts.saturating_sub(publish_time).saturating_mul(1_000);
    require!(elapsed_ms <= max_age_ms as i64, ErrorCode::StalePushPrice);
    Ok(())
}

/// Cross-field validation applied at `create` time (Pyth source only).
pub fn validate_rules(rules: &FeedRules) -> Result<()> {
    if rules.min_price > 0 && rules.max_price > 0 {
        require!(rules.min_price <= rules.max_price, ErrorCode::InvalidRules);
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
    fn deviation_ceiling_clamped_at_100pct() {
        // With 100 bps/hour, after ~100 hours the budget is 100%. Even a
        // doubling should be accepted; a 3× move should not (exceeds diff/last=200%).
        let last: u64 = 1_000_000;
        assert!(check_deviation(2_000_000, last, 1_000, 1_000 + 100 * 3_600, 100).is_ok());
        assert!(check_deviation(3_000_000, last, 1_000, 1_000 + 100 * 3_600, 100).is_err());
    }
}

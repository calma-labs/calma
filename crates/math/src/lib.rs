mod core;
mod traits;

pub use core::*;
pub use traits::*;

const SECONDS_PER_YEAR: u64 = 31_557_600;
pub const PRICE_SCALE: u128 = 1_000_000;

/// Maximum borrow rate the interest math will honor: 10_000% APR
/// (1_000_000 bps). A misconfigured or malicious IRM reporting a higher rate is
/// clamped to this, so interest accrual can never overflow to `None` and brick
/// the pool. The ceiling is far above any plausible crisis rate.
pub const MAX_RATE_BPS: u32 = 1_000_000;

pub enum MathError<E> {
    /// Integer overflow or underflow in a checked arithmetic operation.
    Arithmetic,
    /// Borrow or withdraw would violate the LTV limit.
    Undercollateralized,
    /// Withdrawal exceeds the available balance (collateral deposited or pool shares).
    InsufficientBalance,
    /// The requested amount is too small to mint even one share.
    AmountTooSmall,
    Transfer(E),
}

pub fn utilization_bps(
    total_supply_assets: u64,
    total_borrow_assets: u64,
    assets_in_queue: u64,
) -> u64 {
    if total_supply_assets == 0 {
        return 0;
    }
    let effective_borrowed = total_borrow_assets.saturating_add(assets_in_queue);
    // `effective_borrowed as u128 * 10_000` cannot overflow u128 (operand ≤ u64::MAX),
    // so no masking is needed. If utilization exceeds u64 (only when supply is a
    // tiny fraction of borrowed), saturate high — never report 0, which would
    // falsely signal an idle pool.
    let util = (effective_borrowed as u128) * 10_000 / (total_supply_assets as u128);
    u64::try_from(util).unwrap_or(u64::MAX)
}

pub fn compute_interest(total_borrowed: u64, rate_bps: u32, elapsed_secs: u64) -> Option<u64> {
    if elapsed_secs == 0 || rate_bps == 0 || total_borrowed == 0 {
        return Some(0);
    }
    let rate_bps = rate_bps.min(MAX_RATE_BPS);
    let numerator = (total_borrowed as u128)
        .checked_mul(rate_bps as u128)?
        .checked_mul(elapsed_secs as u128)?;
    let denominator = 10_000u128.checked_mul(SECONDS_PER_YEAR as u128)?;
    let interest = numerator.div_ceil(denominator);
    u64::try_from(interest).ok()
}

/// Call this BEFORE adding `amount` to `total_borrowed`.
///
/// Debt shares are minted with **ceiling** division so a borrower always owes at
/// least their proportional share — rounding favors the protocol, never the
/// borrower. Pairs with the floor rounding in [`amount_to_shares_burned`].
pub fn amount_to_shares(amount: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 && total_borrowed == 0 {
        // Fresh pool: seed debt shares 1:1 with the first borrow.
        return Some(amount);
    }
    if total_debt_shares == 0 || total_borrowed == 0 {
        // Exactly one side is zero — an inconsistent pool (orphan shares with no
        // borrowed assets, or vice versa). Refuse rather than mis-seed 1:1.
        return None;
    }
    let shares = (amount as u128)
        .checked_mul(total_debt_shares as u128)?
        .div_ceil(total_borrowed as u128);
    u64::try_from(shares).ok()
}

/// Uses ceiling division so the protocol never under-collects.
pub fn shares_to_amount(shares: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if shares == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 {
        return None;
    }
    let numer = (shares as u128).checked_mul(total_borrowed as u128)?;
    let result = numer.div_ceil(total_debt_shares as u128);
    u64::try_from(result).ok()
}

pub fn compute_ltv(debt: u64, collateral: u64) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    if collateral == 0 {
        return Some(0);
    }
    let ltv_bps = (debt as u128).checked_mul(10_000)? / (collateral as u128);
    u32::try_from(ltv_bps).ok()
}

pub fn compute_health_factor(collateral: u64, ltv_percent: u8, debt: u64) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    let numerator = (collateral as u128)
        .checked_mul(ltv_percent as u128)?
        .checked_mul(100)?;
    let hf_bps = numerator / (debt as u128);
    u32::try_from(hf_bps).ok()
}

pub fn compute_liquidation_threshold(debt: u64, collateral: u64, ltv_percent: u8) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    if collateral == 0 || ltv_percent == 0 {
        return Some(0);
    }
    let numerator = (debt as u128).checked_mul(1_000_000)?;
    let denominator = (collateral as u128).checked_mul(ltv_percent as u128)?;
    let liq_bps = numerator / denominator;
    u32::try_from(liq_bps).ok()
}

/// Shares burned when repaying `repay_amount`, capped at `max_shares` so a repay
/// never burns more shares than the position holds.
///
/// Uses **floor** division: a partial repay burns no more shares than the tokens
/// received are worth, so the pool never loses (rounding favors the protocol).
/// Pairs with the ceiling rounding in [`amount_to_shares`]. Returns `None` when
/// `total_borrowed == 0` (no debt to burn against) rather than dividing by zero.
pub fn amount_to_shares_burned(
    repay_amount: u64,
    total_borrowed: u64,
    total_debt_shares: u64,
    max_shares: u64,
) -> Option<u64> {
    if repay_amount == 0 {
        return Some(0);
    }
    if total_borrowed == 0 {
        return None;
    }
    let shares = (repay_amount as u128).checked_mul(total_debt_shares as u128)?;
    let shares = shares / (total_borrowed as u128);
    let shares = u64::try_from(shares).ok()?.min(max_shares);
    Some(shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    const YEAR: u64 = SECONDS_PER_YEAR;

    #[test]
    fn zero_elapsed_is_zero_interest() {
        assert_eq!(compute_interest(1_000_000, 500, 0), Some(0));
    }

    #[test]
    fn zero_principal_is_zero_interest() {
        assert_eq!(compute_interest(0, 500, YEAR), Some(0));
    }

    #[test]
    fn zero_rate_is_zero_interest() {
        assert_eq!(compute_interest(1_000_000, 0, YEAR), Some(0));
    }

    #[test]
    fn one_year_hundred_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 10_000, YEAR), Some(1_000_000));
    }

    #[test]
    fn one_year_fifty_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 5_000, YEAR), Some(500_000));
    }

    #[test]
    fn one_year_one_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 100, YEAR), Some(10_000));
    }

    #[test]
    fn half_year_is_half_of_full_year() {
        let full = compute_interest(1_000_000, 5_000, YEAR).unwrap();
        let half = compute_interest(1_000_000, 5_000, YEAR / 2).unwrap();
        assert!((full / 2).abs_diff(half) <= 1);
    }

    #[test]
    fn graceful_none_on_u64_overflow() {
        // u64::MAX × u32::MAX overflows u128 in the second checked_mul → None.
        assert_eq!(compute_interest(u64::MAX, u32::MAX, 1_000 * YEAR), None);
    }

    #[test]
    fn zero_amount_live_pool_is_zero_shares() {
        // Proves the amount == 0 early-return is taken, not the ratio formula.
        assert_eq!(amount_to_shares(0, 1_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn first_borrow_is_one_to_one() {
        assert_eq!(amount_to_shares(500, 0, 0), Some(500));
    }

    #[test]
    fn round_trip_single_borrower() {
        let amount = 1_000_000u64;
        let shares = amount_to_shares(amount, 0, 0).unwrap();
        assert_eq!(shares, amount);
        let back = shares_to_amount(shares, amount, shares).unwrap();
        assert!(back.abs_diff(amount) <= 1);
    }

    #[test]
    fn second_borrow_after_interest_accrual() {
        // Ceiling division rounds the borrower's debt shares up: 90_909.09 → 90_910.
        let shares = amount_to_shares(100_000, 1_100_000, 1_000_000).unwrap();
        assert_eq!(shares, 90_910);
        let new_total_borrowed = 1_100_000 + 100_000;
        let new_total_shares = 1_000_000 + shares;
        let second_debt = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(second_debt.abs_diff(100_000) <= 1);
    }

    #[test]
    fn zero_shares_is_zero_amount() {
        assert_eq!(shares_to_amount(0, 1_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn nonzero_shares_zero_total_shares_returns_none() {
        // Inconsistent state (e.g. stale pool data during a race condition) must not panic.
        assert_eq!(shares_to_amount(1, 1_000_000, 0), None);
    }

    // ── Group A: Minimal amounts (1 base unit) ────────────────────────────────

    #[test]
    fn minimal_first_borrow_one_unit() {
        assert_eq!(amount_to_shares(1, 0, 0), Some(1));
    }

    #[test]
    fn minimal_shares_to_amount_one_unit() {
        assert_eq!(shares_to_amount(1, 1, 1), Some(1));
    }

    #[test]
    fn minimal_repay_burns_one_share() {
        assert_eq!(amount_to_shares_burned(1, 1, 1, 1), Some(1));
    }

    #[test]
    fn minimal_repay_one_unit_from_large_pool() {
        // Ceiling division: 1 unit repaid burns exactly 1 share regardless of pool size.
        const HUNDRED_M: u64 = 100_000_000 * 1_000_000;
        assert_eq!(
            amount_to_shares_burned(1, HUNDRED_M, HUNDRED_M, HUNDRED_M),
            Some(1)
        );
    }

    #[test]
    fn minimal_interest_one_year_100pct() {
        // ceil(1 × 10_000 × YEAR / (10_000 × YEAR)) = ceil(1.0) = 1
        assert_eq!(compute_interest(1, 10_000, YEAR), Some(1));
    }

    #[test]
    fn minimal_interest_one_year_1pct() {
        // ceil(1 × 100 × YEAR / (10_000 × YEAR)) = ceil(0.01) = 1 (rounds up)
        assert_eq!(compute_interest(1, 100, YEAR), Some(1));
    }

    #[test]
    fn minimal_interest_one_second() {
        // ceil(1 × 10_000 × 1 / (10_000 × YEAR)) = ceil(1/YEAR) = 1 (rounds up)
        assert_eq!(compute_interest(1, 10_000, 1), Some(1));
    }

    // ── Group B: Huge amounts (100M tokens = 10^14 base units) ───────────────

    const HUNDRED_M: u64 = 100_000_000 * 1_000_000; // 10^14 base units
    const SEVENTY_FIVE_M: u64 = 75_000_000 * 1_000_000;
    const FIFTY_M: u64 = 50_000_000 * 1_000_000;

    #[test]
    fn huge_first_borrow_100m() {
        assert_eq!(amount_to_shares(HUNDRED_M, 0, 0), Some(HUNDRED_M));
    }

    #[test]
    fn huge_round_trip_single_borrower() {
        let shares = amount_to_shares(HUNDRED_M, 0, 0).unwrap();
        assert_eq!(shares, HUNDRED_M);
        let back = shares_to_amount(shares, HUNDRED_M, shares).unwrap();
        assert_eq!(back, HUNDRED_M);
    }

    #[test]
    fn huge_second_borrow_after_10pct_interest() {
        // Pool has 110M assets and 100M shares after 10% interest accrual.
        // New 100M borrow gets ceil(100M × 100M / 110M) = 90_909_090_909_091 shares
        // (rounded up so the borrower never under-owes).
        let total_after_interest = 110_000_000u64 * 1_000_000;
        let shares = amount_to_shares(HUNDRED_M, total_after_interest, HUNDRED_M).unwrap();
        assert_eq!(shares, 90_909_090_909_091);

        // Ceiling on repay brings it back within 1 unit of the original borrow.
        let new_total_borrowed = total_after_interest + HUNDRED_M;
        let new_total_shares = HUNDRED_M + shares;
        let back = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(back.abs_diff(HUNDRED_M) <= 1);
    }

    #[test]
    fn huge_interest_10pct_one_year() {
        let interest = compute_interest(HUNDRED_M, 1_000, YEAR).unwrap();
        assert_eq!(interest, 10_000_000 * 1_000_000);
    }

    #[test]
    fn huge_interest_100pct_one_year() {
        assert_eq!(compute_interest(HUNDRED_M, 10_000, YEAR), Some(HUNDRED_M));
    }

    #[test]
    fn huge_flash_fee() {
        // 9 bps of 100M tokens = 90_000 tokens = 90_000_000_000 base units.
        assert_eq!(flash_fee(HUNDRED_M), Some(90_000_000_000));
    }

    #[test]
    fn huge_compute_ltv_at_75pct() {
        // 75M debt / 100M collateral = 7500 bps.
        assert_eq!(compute_ltv(SEVENTY_FIVE_M, HUNDRED_M), Some(7_500));
    }

    #[test]
    fn huge_health_factor_at_liquidation_limit() {
        // collateral=100M, ltv=75%, debt=75M → HF = 100M×75×100/75M = 10_000 (exactly at limit).
        assert_eq!(
            compute_health_factor(HUNDRED_M, 75, SEVENTY_FIVE_M),
            Some(10_000)
        );
    }

    #[test]
    fn huge_health_factor_healthy() {
        // collateral=100M, ltv=75%, debt=50M → HF = 100M×75×100/50M = 15_000.
        assert_eq!(
            compute_health_factor(HUNDRED_M, 75, FIFTY_M),
            Some(15_000)
        );
    }

    #[test]
    fn huge_full_repay() {
        // Repaying the full 100M against a 100M pool burns all 100M shares.
        assert_eq!(
            amount_to_shares_burned(HUNDRED_M, HUNDRED_M, HUNDRED_M, HUNDRED_M),
            Some(HUNDRED_M)
        );
    }

    // ── Group C: Near-u64::MAX — must succeed ─────────────────────────────────

    #[test]
    fn interest_exact_u64_max_boundary() {
        // numerator = u64::MAX × 10_000 × YEAR; denominator = 10_000 × YEAR.
        // interest = ceil(u64::MAX) = u64::MAX — fits exactly in u64.
        assert_eq!(compute_interest(u64::MAX, 10_000, YEAR), Some(u64::MAX));
    }

    #[test]
    fn interest_u64_max_tiny_rate_one_sec() {
        // u64::MAX × 500 / (10_000 × YEAR) ≈ 29 billion — fits well in u64.
        let v = compute_interest(u64::MAX, 500, 1).unwrap();
        assert!(v > 0 && v < u64::MAX);
    }

    #[test]
    fn flash_fee_u64_max_succeeds() {
        // 9 × u64::MAX fits in u128; ceil(… / 10_000) fits back in u64.
        let expected = u64::try_from(
            (u64::MAX as u128).checked_mul(9).unwrap().div_ceil(10_000)
        ).unwrap();
        assert_eq!(flash_fee(u64::MAX), Some(expected));
    }

    // ── Group D: Must return None — no panic, no incorrect result ─────────────

    #[test]
    fn interest_one_second_over_u64_max_returns_none() {
        // YEAR+1 seconds: interest = u64::MAX + ~584M > u64::MAX → try_from fails.
        assert_eq!(compute_interest(u64::MAX, 10_000, YEAR + 1), None);
    }

    #[test]
    fn amount_to_shares_result_overflows_u64_returns_none() {
        // u64::MAX × u64::MAX fits u128 but the quotient won't fit u64.
        assert_eq!(amount_to_shares(u64::MAX, 1, u64::MAX), None);
    }

    #[test]
    fn shares_to_amount_result_overflows_u64_returns_none() {
        assert_eq!(shares_to_amount(u64::MAX, u64::MAX, 1), None);
    }

    #[test]
    fn amount_to_shares_burned_overflow_returns_none() {
        assert_eq!(
            amount_to_shares_burned(u64::MAX, 1, u64::MAX, u64::MAX),
            None
        );
    }

    #[test]
    fn health_factor_overflows_u32_returns_none() {
        // collateral=u64::MAX, ltv=100, debt=1 → HF ≫ u32::MAX → None.
        assert_eq!(compute_health_factor(u64::MAX, 100, 1), None);
    }

    #[test]
    fn health_factor_zero_debt_returns_none() {
        // debt == 0 is the early-return guard; removing it would cause a divide-by-zero panic.
        assert_eq!(compute_health_factor(1_000_000, 75, 0), None);
    }

    // ── compute_ltv missing None test ─────────────────────────────────────────

    #[test]
    fn compute_ltv_overflows_u32_returns_none() {
        // ltv_bps = u64::MAX × 10_000 / 1 ≈ 1.84×10²³ >> u32::MAX → try_from fails.
        assert_eq!(compute_ltv(u64::MAX, 1), None);
    }

    // ── compute_liquidation_threshold — full four-band coverage ───────────────

    #[test]
    fn liq_threshold_zero_debt_returns_none() {
        assert_eq!(compute_liquidation_threshold(0, 1_000_000, 75), None);
    }

    #[test]
    fn liq_threshold_zero_collateral_returns_zero() {
        assert_eq!(compute_liquidation_threshold(1_000_000, 0, 75), Some(0));
    }

    #[test]
    fn liq_threshold_zero_ltv_returns_zero() {
        assert_eq!(compute_liquidation_threshold(1_000_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn liq_threshold_at_exact_ltv_returns_price_scale() {
        // debt=75M, collateral=100M, ltv=75 → liq_price = 75M×1_000_000 / (100M×75) = 10_000.
        assert_eq!(
            compute_liquidation_threshold(SEVENTY_FIVE_M, HUNDRED_M, 75),
            Some(10_000)
        );
    }

    #[test]
    fn liq_threshold_overflows_u32_returns_none() {
        // liq_bps = u64::MAX × 1_000_000 / (1 × 1) >> u32::MAX → None.
        assert_eq!(compute_liquidation_threshold(u64::MAX, 1, 1), None);
    }

    // ── Rounding direction: shares must always favor the protocol ─────────────

    #[test]
    fn amount_to_shares_burned_zero_borrowed_returns_none() {
        // total_borrowed == 0 used to divide by zero (panic). Must return None.
        assert_eq!(amount_to_shares_burned(1, 0, 1_000_000, 1_000_000), None);
    }

    #[test]
    fn borrow_shares_round_up() {
        // 1 unit into a pool where 1 share is worth 2 units: exact = 0.5 share.
        // Ceiling mints 1 share so the borrower never owes 0 for a real borrow.
        assert_eq!(amount_to_shares(1, 2, 1), Some(1));
    }

    #[test]
    fn repay_shares_burned_round_down() {
        // Repay 1 unit where 1 share is worth 2 units: exact = 0.5 share.
        // Floor burns 0 shares so the pool never releases more debt than paid for.
        assert_eq!(amount_to_shares_burned(1, 2, 1, 1), Some(0));
    }

    #[test]
    fn borrow_owes_at_least_what_was_borrowed() {
        // Ceiling mint + ceiling valuation guarantee the debt never rounds below
        // the borrowed amount — the invariant that keeps the pool fully backed.
        let borrowed = 100_000u64;
        let (tb, ts) = (1_100_000u64, 1_000_000u64);
        let shares = amount_to_shares(borrowed, tb, ts).unwrap();
        let owed = shares_to_amount(shares, tb + borrowed, ts + shares).unwrap();
        assert!(owed >= borrowed, "owed {owed} < borrowed {borrowed}");
    }

    // ── flash_fee zero and minimal ─────────────────────────────────────────────

    #[test]
    fn flash_fee_zero_amount_is_zero() {
        assert_eq!(flash_fee(0), Some(0));
    }

    #[test]
    fn flash_fee_tiny_amount_charges_minimum_one() {
        // Ceiling + min(1): a nonzero flash loan is never free.
        // 1 × 9 = 9 → ceil(9/10_000) = 1; 1_111 × 9 = 9_999 → ceil = 1.
        assert_eq!(flash_fee(1), Some(1));
        assert_eq!(flash_fee(1_111), Some(1));
    }

    #[test]
    fn flash_fee_rounds_up() {
        // 1_112 × 9 = 10_008 → ceil(10_008 / 10_000) = 2 (was 1 under floor).
        assert_eq!(flash_fee(1_112), Some(2));
    }

    // ── M5: interest-rate ceiling ─────────────────────────────────────────────

    #[test]
    fn interest_rate_above_max_is_clamped() {
        // A rate 10× over the ceiling yields the same interest as the ceiling —
        // accrual never overflows to None, so the pool can't be bricked.
        let at_cap = compute_interest(1_000_000, MAX_RATE_BPS, YEAR);
        let over = compute_interest(1_000_000, MAX_RATE_BPS * 10, YEAR);
        assert_eq!(over, at_cap);
        // 10_000% APR on 1_000_000 principal for one year = 100_000_000.
        assert_eq!(at_cap, Some(100_000_000));
    }

    // ── L2: utilization ───────────────────────────────────────────────────────

    #[test]
    fn utilization_half_and_zero_supply() {
        assert_eq!(utilization_bps(1_000_000, 500_000, 0), 5_000);
        assert_eq!(utilization_bps(1_000_000, 400_000, 100_000), 5_000); // queue counts
        assert_eq!(utilization_bps(0, 500_000, 0), 0);
    }

    // ── L4: amount_to_shares rejects inconsistent one-sided-zero state ────────

    #[test]
    fn amount_to_shares_orphan_shares_no_borrowed_returns_none() {
        // shares outstanding but zero borrowed assets — inconsistent, reject.
        assert_eq!(amount_to_shares(500, 0, 1_000_000), None);
        // borrowed assets but zero shares — equally inconsistent.
        assert_eq!(amount_to_shares(500, 1_000_000, 0), None);
    }
}

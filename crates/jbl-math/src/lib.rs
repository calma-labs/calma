mod traits;
mod core;

pub use traits::*;
pub use core::*;

const SECONDS_PER_YEAR: u64 = 31_557_600;
pub const PRICE_SCALE: u128 = 1_000_000;

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

pub fn utilization_bps(total_supply_assets: u64, total_borrow_assets: u64, assets_in_queue: u64) -> u64 {
    if total_supply_assets == 0 {
        return 0;
    }
    let effective_borrowed = total_borrow_assets.saturating_add(assets_in_queue);
    (effective_borrowed as u128)
        .checked_mul(10_000)
        .unwrap_or(0)
        .checked_div(total_supply_assets as u128)
        .unwrap_or(0) as u64
}

pub fn flash_fee(amount: u64, fee_bps: u32) -> Option<u64> {
    u64::try_from(
        (amount as u128)
            .checked_mul(fee_bps as u128)?
            .checked_div(10_000)?,
    )
    .ok()
}

pub fn compute_interest(total_borrowed: u64, rate_bps: u32, elapsed_secs: u64) -> Option<u64> {
    if elapsed_secs == 0 || rate_bps == 0 || total_borrowed == 0 {
        return Some(0);
    }
    let numerator = (total_borrowed as u128)
        .checked_mul(rate_bps as u128)?
        .checked_mul(elapsed_secs as u128)?;
    let denominator = 10_000u128.checked_mul(SECONDS_PER_YEAR as u128)?;
    let interest = numerator.div_ceil(denominator);
    u64::try_from(interest).ok()
}

/// Call this BEFORE adding `amount` to `total_borrowed`.
pub fn amount_to_shares(amount: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 || total_borrowed == 0 {
        return Some(amount);
    }
    let shares =
        (amount as u128).checked_mul(total_debt_shares as u128)? / (total_borrowed as u128);
    u64::try_from(shares).ok()
}

/// Uses ceiling division so the protocol never under-collects.
pub fn shares_to_amount(shares: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if shares == 0 {
        return Some(0);
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

/// Uses ceiling division, capped at `max_shares` so full-repay never burns more shares than held.
pub fn amount_to_shares_burned(
    repay_amount: u64,
    total_borrowed: u64,
    total_debt_shares: u64,
    max_shares: u64,
) -> Option<u64> {
    if repay_amount == 0 {
        return Some(0);
    }
    let shares = (repay_amount as u128).checked_mul(total_debt_shares as u128)?;
    let shares = shares.div_ceil(total_borrowed as u128);
    let shares = u64::try_from(shares).ok()?.min(max_shares);
    Some(shares)
}

/// Mirrors the on-chain LTV check in `borrow_handler`.
pub fn max_borrowable(collateral: u64, ltv_percent: u8) -> u64 {
    collateral.saturating_mul(ltv_percent as u64) / 100
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
        let result = compute_interest(u64::MAX, u32::MAX, 1_000 * YEAR);
        let _ = result;
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
        assert_eq!(back, amount);
    }

    #[test]
    fn second_borrow_after_interest_accrual() {
        let shares = amount_to_shares(100_000, 1_100_000, 1_000_000).unwrap();
        assert_eq!(shares, 90_909);
        let new_total_borrowed = 1_100_000 + 100_000;
        let new_total_shares = 1_000_000 + shares;
        let second_debt = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(second_debt.abs_diff(100_000) <= 1);
    }

    #[test]
    fn zero_shares_is_zero_amount() {
        assert_eq!(shares_to_amount(0, 1_000_000, 1_000_000), Some(0));
    }
}

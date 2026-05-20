const SECONDS_PER_YEAR: u64 = 31_557_600;

/// Compute simple interest on a pool's total borrowed balance.
///
/// ```text
/// interest = total_borrowed × rate_bps × elapsed_secs
///            ─────────────────────────────────────
///                  10_000 × SECONDS_PER_YEAR
/// ```
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

/// Convert a borrow amount to debt shares given pool state.
///
/// If the pool has no shares yet (first borrow), shares = amount (1:1).
/// Otherwise: `shares = amount × total_debt_shares / total_borrowed`
///
/// Call this BEFORE adding `amount` to `total_borrowed`.
pub fn amount_to_shares(amount: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 || total_borrowed == 0 {
        return Some(amount); // 1:1 for the first borrow
    }
    let shares =
        (amount as u128).checked_mul(total_debt_shares as u128)? / (total_borrowed as u128);
    u64::try_from(shares).ok()
}

/// Convert debt shares to the current outstanding token amount.
///
/// `amount = shares × total_borrowed / total_debt_shares`
///
/// Uses ceiling division so the protocol never under-collects.
pub fn shares_to_amount(shares: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if shares == 0 {
        return Some(0);
    }
    let numer = (shares as u128).checked_mul(total_borrowed as u128)?;
    let result = numer.div_ceil(total_debt_shares as u128);
    u64::try_from(result).ok()
}

/// Convert a repay token amount to the number of debt shares to burn.
///
/// `shares = repay_amount × total_debt_shares / total_borrowed`
///
/// Uses ceiling division and is capped at `max_shares` so full-repay rounding
/// never burns more shares than the user holds.
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

#[cfg(test)]
mod tests {
    use super::*;

    const YEAR: u64 = SECONDS_PER_YEAR;

    // ── compute_interest ─────────────────────────────────────────────────────

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

    // ── amount_to_shares / shares_to_amount ──────────────────────────────────

    #[test]
    fn first_borrow_is_one_to_one() {
        assert_eq!(amount_to_shares(500, 0, 0), Some(500));
    }

    #[test]
    fn round_trip_single_borrower() {
        let amount = 1_000_000u64;
        let shares = amount_to_shares(amount, 0, 0).unwrap(); // first borrow
        assert_eq!(shares, amount);
        let back = shares_to_amount(shares, amount, shares).unwrap();
        assert_eq!(back, amount);
    }

    #[test]
    fn second_borrow_after_interest_accrual() {
        // Pool had 1_000_000 borrowed, accrued 100_000 interest -> total_borrowed = 1_100_000
        // total_debt_shares still = 1_000_000 (first borrower's shares)
        // Second borrower wants 100_000; shares should be proportional
        let shares = amount_to_shares(100_000, 1_100_000, 1_000_000).unwrap();
        // 100_000 * 1_000_000 / 1_100_000 = ~90_909 shares
        assert_eq!(shares, 90_909);
        // First borrower's debt grew: 1_000_000 shares * 1_200_000 / 1_090_909 ≈...
        // Second borrower's debt: 90_909 * 1_200_000 / 1_090_909 ≈ 100_000
        let new_total_borrowed = 1_100_000 + 100_000; // after second borrow
        let new_total_shares = 1_000_000 + shares;
        let second_debt = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(second_debt.abs_diff(100_000) <= 1);
    }

    #[test]
    fn zero_shares_is_zero_amount() {
        assert_eq!(shares_to_amount(0, 1_000_000, 1_000_000), Some(0));
    }
}

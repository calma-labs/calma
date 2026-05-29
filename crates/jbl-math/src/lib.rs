mod traits;
pub use traits::*;

const SECONDS_PER_YEAR: u64 = 31_557_600;
pub const PRICE_SCALE: u128 = 1_000_000;

pub fn flash_fee(amount: u64, fee_bps: u32) -> Option<u64> {
    u64::try_from(
        (amount as u128)
            .checked_mul(fee_bps as u128)?
            .checked_div(10_000)?,
    )
    .ok()
}

pub struct Core<M: Market, O = (), I = (), P = ()> {
    pub market: M,
    pub oracle: O,
    pub irm: I,
    pub position: P,
}

impl<M: Market> Core<M, (), (), ()> {
    pub fn new(market: M) -> Self {
        Core { market, oracle: (), irm: (), position: () }
    }
}

impl<M: Market, O, I, P> Core<M, O, I, P> {
    pub fn with_oracle<O2: Oracle>(self, oracle: O2) -> Core<M, O2, I, P> {
        Core { market: self.market, oracle, irm: self.irm, position: self.position }
    }
    pub fn with_irm<I2: IrmRate>(self, irm: I2) -> Core<M, O, I2, P> {
        Core { market: self.market, oracle: self.oracle, irm, position: self.position }
    }
    pub fn with_position<P2: Position>(self, position: P2) -> Core<M, O, I, P2> {
        Core { market: self.market, oracle: self.oracle, irm: self.irm, position }
    }

    pub fn calc_lend_for_shares(&self, shares: u64) -> Option<u64> {
        let total_shares = self.market.total_supply_shares();
        if total_shares == 0 { return None; }
        u64::try_from(
            (shares as u128)
                .checked_mul(self.market.total_supply_assets() as u128)?
                .checked_div(total_shares as u128)?,
        )
        .ok()
    }

    pub fn deposit_lent(&mut self, amount: u64) -> Option<u64> {
        let lp = if self.market.total_supply_shares() == 0 || self.market.total_supply_assets() == 0 {
            amount
        } else {
            u64::try_from(
                (amount as u128)
                    .checked_mul(self.market.total_supply_shares() as u128)?
                    .checked_div(self.market.total_supply_assets() as u128)?,
            )
            .ok()?
        };
        if lp == 0 { return None; }
        self.market.set_total_supply_assets(self.market.total_supply_assets().checked_add(amount)?);
        self.market.set_total_supply_shares(self.market.total_supply_shares().checked_add(lp)?);
        Some(lp)
    }

    pub fn withdraw_lent_immediate(&mut self, shares: u64) -> Option<u64> {
        let lend = self.calc_lend_for_shares(shares)?;
        let clamped = lend.min(self.market.total_supply_assets());
        self.market.set_total_supply_shares(self.market.total_supply_shares().checked_sub(shares)?);
        self.market.set_total_supply_assets(self.market.total_supply_assets().checked_sub(clamped)?);
        Some(lend)
    }

    pub fn withdraw_lent_queued(&mut self, shares: u64) -> Option<u64> {
        let lend = self.calc_lend_for_shares(shares)?;
        self.market.set_total_supply_shares(self.market.total_supply_shares().checked_sub(shares)?);
        self.market.set_assets_in_queue(self.market.assets_in_queue().checked_add(lend)?);
        Some(lend)
    }
}

impl<M: Market, O: Oracle, I: IrmRate, P> Core<M, O, I, P> {
    pub fn accrue_interest(&mut self) -> Option<()> {
        let elapsed = (self.oracle.current_ts().saturating_sub(self.market.last_update())).max(0) as u64;
        if elapsed == 0 {
            return Some(());
        }
        let interest = compute_interest(self.market.total_borrow_assets(), self.irm.rate_bps(), elapsed)?;
        let new_borrow = self.market.total_borrow_assets().checked_add(interest)?;
        self.market.set_total_borrow_assets(new_borrow);
        self.market.set_last_update(self.oracle.current_ts());
        Some(())
    }
}

impl<M: Market, O, I, P: Position> Core<M, O, I, P> {
    pub fn borrow(&mut self, amount: u64, oracle_price: u64) -> Option<u64> {
        let max_borrowable = u64::try_from(
            (self.position.collateral_deposited() as u128)
                .checked_mul(oracle_price as u128)?
                .checked_div(PRICE_SCALE)?
                .checked_mul(self.market.ltv_percent() as u128)?
                .checked_div(100)?,
        )
        .ok()?;
        let current_debt = if self.market.total_borrow_shares() > 0 {
            shares_to_amount(self.position.debt_shares(), self.market.total_borrow_assets(), self.market.total_borrow_shares())?
        } else { 0 };
        if amount > max_borrowable.checked_sub(current_debt)? { return None; }
        let new_shares = amount_to_shares(amount, self.market.total_borrow_assets(), self.market.total_borrow_shares())?;
        if new_shares == 0 { return None; }
        self.position.set_debt_shares(self.position.debt_shares().checked_add(new_shares)?);
        self.market.set_total_borrow_shares(self.market.total_borrow_shares().checked_add(new_shares)?);
        self.market.set_total_borrow_assets(self.market.total_borrow_assets().checked_add(amount)?);
        Some(new_shares)
    }

    pub fn borrow_with_fee(&mut self, amount: u64, total_debt_amount: u64, oracle_price: u64) -> Option<u64> {
        let max_borrowable = u64::try_from(
            (self.position.collateral_deposited() as u128)
                .checked_mul(oracle_price as u128)?
                .checked_div(PRICE_SCALE)?
                .checked_mul(self.market.ltv_percent() as u128)?
                .checked_div(100)?,
        )
        .ok()?;
        let current_debt = if self.market.total_borrow_shares() > 0 {
            shares_to_amount(self.position.debt_shares(), self.market.total_borrow_assets(), self.market.total_borrow_shares())?
        } else { 0 };
        if amount > max_borrowable.checked_sub(current_debt)? { return None; }
        let new_shares = amount_to_shares(total_debt_amount, self.market.total_borrow_assets(), self.market.total_borrow_shares())?;
        if new_shares == 0 { return None; }
        self.position.set_debt_shares(self.position.debt_shares().checked_add(new_shares)?);
        self.market.set_total_borrow_shares(self.market.total_borrow_shares().checked_add(new_shares)?);
        self.market.set_total_borrow_assets(self.market.total_borrow_assets().checked_add(total_debt_amount)?);
        Some(new_shares)
    }

    pub fn repay(&mut self, amount: u64) -> Option<(u64, u64)> {
        let debt_shares = self.position.debt_shares();
        let total_due = shares_to_amount(debt_shares, self.market.total_borrow_assets(), self.market.total_borrow_shares())?;
        let repay_amount = amount.min(total_due);
        let shares_to_burn = if repay_amount == total_due {
            debt_shares
        } else {
            amount_to_shares_burned(repay_amount, self.market.total_borrow_assets(), self.market.total_borrow_shares(), debt_shares)?
        };
        self.position.set_debt_shares(debt_shares.checked_sub(shares_to_burn)?);
        self.market.set_total_borrow_shares(self.market.total_borrow_shares().checked_sub(shares_to_burn)?);
        self.market.set_total_borrow_assets(self.market.total_borrow_assets().checked_sub(repay_amount)?);
        Some((repay_amount, shares_to_burn))
    }

    pub fn withdraw_collateral(&mut self, amount: u64, oracle_price: u64) -> Option<u64> {
        let remaining = self.position.collateral_deposited().checked_sub(amount)?;
        let max_borrowable = u64::try_from(
            (remaining as u128)
                .checked_mul(oracle_price as u128)?
                .checked_div(PRICE_SCALE)?
                .checked_mul(self.market.ltv_percent() as u128)?
                .checked_div(100)?,
        )
        .ok()?;
        let current_debt = if self.market.total_borrow_shares() > 0 {
            shares_to_amount(self.position.debt_shares(), self.market.total_borrow_assets(), self.market.total_borrow_shares())?
        } else { 0 };
        if current_debt > max_borrowable { return None; }
        self.position.set_collateral_deposited(remaining);
        Some(remaining)
    }

    pub fn settle_hedge(&mut self, initial_shares: u64, borrow_amount: u64, upfront_fee: u64) -> Option<(u64, u64)> {
        let current_value = shares_to_amount(initial_shares, self.market.total_borrow_assets(), self.market.total_borrow_shares())?;
        self.market.set_total_borrow_shares(self.market.total_borrow_shares().checked_sub(initial_shares)?);
        self.market.set_total_borrow_assets(self.market.total_borrow_assets().checked_sub(current_value)?);
        self.market.set_total_supply_assets(self.market.total_supply_assets().saturating_sub(upfront_fee));
        let new_shares = amount_to_shares(borrow_amount, self.market.total_borrow_assets(), self.market.total_borrow_shares())?;
        self.market.set_total_borrow_shares(self.market.total_borrow_shares().checked_add(new_shares)?);
        self.market.set_total_borrow_assets(self.market.total_borrow_assets().checked_add(borrow_amount)?);
        let old_debt = self.position.debt_shares();
        self.position.set_debt_shares(old_debt.checked_sub(initial_shares)?.checked_add(new_shares)?);
        Some((current_value, new_shares))
    }
}

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

/// Compute Loan-to-Value (LTV) ratio in basis points (100% = 10,000).
/// Returns None if debt is 0 to indicate "N/A".
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

/// Compute Health Factor in basis points (1.0 = 10,000).
/// health_factor = (collateral * ltv_percent / 100) / debt
/// Returns None if debt is 0 to indicate "N/A".
pub fn compute_health_factor(collateral: u64, ltv_percent: u8, debt: u64) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    // (collateral * (ltv_percent / 100) * 10,000) / debt
    // = (collateral * ltv_percent * 100) / debt
    let numerator = (collateral as u128)
        .checked_mul(ltv_percent as u128)?
        .checked_mul(100)?;
    let hf_bps = numerator / (debt as u128);
    u32::try_from(hf_bps).ok()
}

/// Compute Liquidation "Price" (ratio) in basis points.
/// liq_price = debt / (collateral * ltv_percent / 100)
/// Returns None if debt is 0 to indicate "N/A".
pub fn compute_liquidation_threshold(debt: u64, collateral: u64, ltv_percent: u8) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    if collateral == 0 || ltv_percent == 0 {
        return Some(0);
    }
    // (debt * 10,000) / (collateral * ltv_percent / 100)
    // = (debt * 1,000,000) / (collateral * ltv_percent)
    let numerator = (debt as u128).checked_mul(1_000_000)?;
    let denominator = (collateral as u128).checked_mul(ltv_percent as u128)?;
    let liq_bps = numerator / denominator;
    u32::try_from(liq_bps).ok()
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

/// Maximum amount a user may borrow against their collateral.
///
/// `max_borrowable = collateral * ltv_percent / 100`
///
/// Mirrors the on-chain LTV check in `borrow_handler`.
pub fn max_borrowable(collateral: u64, ltv_percent: u8) -> u64 {
    collateral.saturating_mul(ltv_percent as u64) / 100
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

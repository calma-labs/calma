use std::marker::PhantomData;

use crate::{
    amount_to_shares, amount_to_shares_burned, compute_interest, shares_to_amount, IrmRate, Market,
    MathError, Oracle, Position, PRICE_SCALE,
};

/// Flash loan fee in basis points (9 bps = 0.09%). Single source of truth shared
/// by the on-chain program and the client bindings.
pub const FLASH_LOAN_FEE_BPS: u64 = 9;

/// Flash-loan fee owed on `amount` at the protocol flash-fee rate.
///
/// Rounds **up** and charges at least 1 base unit on any nonzero borrow, so a
/// flash loan is never free (floor rounding previously made sub-1112-unit loans
/// cost nothing).
pub fn flash_fee(amount: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    let fee = (amount as u128)
        .checked_mul(FLASH_LOAN_FEE_BPS as u128)?
        .div_ceil(10_000)
        .max(1);
    u64::try_from(fee).ok()
}

pub struct NotAccrued;
pub struct Accrued;

pub struct Core<M: Market, I = (), P = (), O = (), S = NotAccrued> {
    pub market: M,
    pub irm: I,
    pub position: P,
    pub oracle: O,
    _state: PhantomData<S>,
}

impl<M: Market> Core<M, (), (), (), NotAccrued> {
    pub fn new(market: M) -> Self {
        Core {
            market,
            irm: (),
            position: (),
            oracle: (),
            _state: PhantomData,
        }
    }
}

impl<M: Market, I, P, O, S> Core<M, I, P, O, S> {
    pub fn with_irm<I2: IrmRate>(self, irm: I2) -> Core<M, I2, P, O, S> {
        Core {
            market: self.market,
            irm,
            position: self.position,
            oracle: self.oracle,
            _state: PhantomData,
        }
    }
    pub fn with_position<P2: Position>(self, position: P2) -> Core<M, I, P2, O, S> {
        Core {
            market: self.market,
            irm: self.irm,
            position,
            oracle: self.oracle,
            _state: PhantomData,
        }
    }
    pub fn with_oracle<O2: Oracle>(self, oracle: O2) -> Core<M, I, P, O2, S> {
        Core {
            market: self.market,
            irm: self.irm,
            position: self.position,
            oracle,
            _state: PhantomData,
        }
    }

    pub fn calc_lend_for_shares(&self, shares: u64) -> Option<u64> {
        let total_shares = self.market.total_supply_shares();
        if total_shares == 0 {
            return None;
        }
        u64::try_from(
            (shares as u128)
                .checked_mul(self.market.total_supply_assets() as u128)?
                .checked_div(total_shares as u128)?,
        )
        .ok()
    }

    /// Share-inflation posture: the classic ERC-4626 donation/inflation attack
    /// is closed here because `total_supply_assets` is **internally accounted**
    /// (updated only by this crate), not read from the vault's token balance — a
    /// direct token transfer into the vault cannot move the share price. The
    /// `lp == 0` guard below (and the `new_shares == 0` guard in `borrow`) reject
    /// zero-share griefing. No virtual-shares offset is applied; deposits round
    /// down and the first depositor seeds 1:1. If the vault balance ever becomes
    /// the source of truth for `total_supply_assets`, add a virtual offset first.
    pub fn deposit_lent<E, F1, F2>(
        &mut self,
        amount: u64,
        transfer: F1,
        mint: F2,
    ) -> Result<u64, MathError<E>>
    where
        F1: FnOnce(u64) -> Result<(), E>,
        F2: FnOnce(u64) -> Result<(), E>,
    {
        let lp = if self.market.total_supply_shares() == 0 || self.market.total_supply_assets() == 0
        {
            amount
        } else {
            u64::try_from(
                (amount as u128)
                    .checked_mul(self.market.total_supply_shares() as u128)
                    .ok_or(MathError::Arithmetic)?
                    .checked_div(self.market.total_supply_assets() as u128)
                    .ok_or(MathError::Arithmetic)?,
            )
            .map_err(|_| MathError::Arithmetic)?
        };
        if lp == 0 {
            return Err(MathError::AmountTooSmall);
        }
        *self.market.total_supply_assets_mut() = self
            .market
            .total_supply_assets()
            .checked_add(amount)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_supply_shares_mut() = self
            .market
            .total_supply_shares()
            .checked_add(lp)
            .ok_or(MathError::Arithmetic)?;
        transfer(amount).map_err(MathError::Transfer)?;
        mint(lp).map_err(MathError::Transfer)?;
        Ok(lp)
    }

    pub fn withdraw_lent_immediate<E, F>(
        &mut self,
        shares: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let lend = self
            .calc_lend_for_shares(shares)
            .ok_or(MathError::InsufficientBalance)?;
        *self.market.total_supply_shares_mut() = self
            .market
            .total_supply_shares()
            .checked_sub(shares)
            .ok_or(MathError::Arithmetic)?;
        // `lend` is always ≤ total_supply_assets for valid shares (floor of
        // shares × assets / shares_total). The `checked_sub` guards the
        // inconsistent case instead of silently transferring `lend` while only
        // debiting a clamped-down amount — which the old `min` clamp allowed.
        *self.market.total_supply_assets_mut() = self
            .market
            .total_supply_assets()
            .checked_sub(lend)
            .ok_or(MathError::Arithmetic)?;
        transfer(lend).map_err(MathError::Transfer)?;
        Ok(lend)
    }

    pub fn withdraw_lent_queued(&mut self, shares: u64) -> Option<u64> {
        let lend = self.calc_lend_for_shares(shares)?;
        *self.market.total_supply_shares_mut() =
            self.market.total_supply_shares().checked_sub(shares)?;
        *self.market.assets_in_queue_mut() = self.market.assets_in_queue().checked_add(lend)?;
        Some(lend)
    }
}

impl<M: Market, I, P: Position, O, S> Core<M, I, P, O, S> {
    pub fn deposit_collateral<E, F>(&mut self, amount: u64, transfer: F) -> Result<(), MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        *self.position.collateral_deposited_mut() = self
            .position
            .collateral_deposited()
            .checked_add(amount)
            .ok_or(MathError::Arithmetic)?;
        transfer(amount).map_err(MathError::Transfer)
    }

    /// Maximum borrowable amount against `collateral` at the market LTV and
    /// `oracle_price`, in lend units.
    ///
    /// Uses **saturating** arithmetic and clamps to `u64::MAX` on overflow (only
    /// reachable at astronomically large collateral). This is the capacity
    /// *ceiling* the LTV gate compares against, so clamping high is safe — the
    /// actual borrow is itself a `u64` and other checks (vault balance) still
    /// apply — and it avoids bricking a large-but-valid position with an
    /// arithmetic error. The intermediate `collateral × oracle_price` cannot
    /// overflow u128 (both operands ≤ `u64::MAX`).
    pub fn max_borrow_capacity(&self, collateral: u64, oracle_price: u64) -> u64 {
        let capacity = (collateral as u128).saturating_mul(oracle_price as u128)
            / PRICE_SCALE
            * (self.market.ltv_percent() as u128)
            / 100;
        u64::try_from(capacity).unwrap_or(u64::MAX)
    }

    fn current_debt_amount(&self) -> Option<u64> {
        if self.market.total_borrow_shares() == 0 {
            Some(0)
        } else {
            shares_to_amount(
                self.position.debt_shares(),
                self.market.total_borrow_assets(),
                self.market.total_borrow_shares(),
            )
        }
    }
}

// ── Typestate transition: NotAccrued → Accrued ────────────────────────────────

impl<M: Market, I: IrmRate, P, O> Core<M, I, P, O, NotAccrued> {
    pub fn accrue_interest(mut self) -> Option<Core<M, I, P, O, Accrued>> {
        let elapsed = (self
            .irm
            .current_ts()
            .saturating_sub(self.market.last_update()))
        .max(0) as u64;
        if elapsed > 0 {
            let interest = compute_interest(
                self.market.total_borrow_assets(),
                self.irm.rate_bps(),
                elapsed,
            )?;
            let new_borrow = self.market.total_borrow_assets().checked_add(interest)?;
            *self.market.total_borrow_assets_mut() = new_borrow;
            // Credit the full accrued interest to the supply side so the lender
            // (LP) share price rises with borrower-paid interest. Without this,
            // interest tokens repaid into the vault are unaccounted and lenders
            // earn zero yield. Both sides move by the identical `interest`, so
            // the books stay exactly balanced.
            let supply_assets = self.market.total_supply_assets().checked_add(interest)?;
            *self.market.total_supply_assets_mut() = supply_assets;

            // Protocol fee (Morpho-style): mint fee shares worth `fee_amount` to
            // the protocol so the cut is held as supply shares, diluting existing
            // lenders only by the fee fraction. `total_supply_assets` already
            // holds the full interest, so the books stay balanced; the shares are
            // tracked in `accrued_fee_shares` until claimed as LP.
            let fee_bps = self.market.fee();
            if fee_bps > 0 && interest > 0 {
                let fee_amount = (interest as u128)
                    .checked_mul(fee_bps as u128)?
                    .checked_div(10_000)?;
                // Shares are valued against supply *excluding* the fee assets.
                let denom = (supply_assets as u128).checked_sub(fee_amount)?;
                if fee_amount > 0 && denom > 0 {
                    let supply_shares = self.market.total_supply_shares();
                    let fee_shares = u64::try_from(
                        fee_amount
                            .checked_mul(supply_shares as u128)?
                            .checked_div(denom)?,
                    )
                    .ok()?;
                    if fee_shares > 0 {
                        *self.market.total_supply_shares_mut() =
                            supply_shares.checked_add(fee_shares)?;
                        *self.market.accrued_fee_shares_mut() =
                            self.market.accrued_fee_shares().checked_add(fee_shares)?;
                    }
                }
            }
            *self.market.last_update_mut() = self.irm.current_ts();
        }
        Some(Core {
            market: self.market,
            irm: self.irm,
            position: self.position,
            oracle: self.oracle,
            _state: PhantomData,
        })
    }
}

// ── Methods available after accrual, any oracle ───────────────────────────────

impl<M: Market, I, P: Position, O> Core<M, I, P, O, Accrued> {
    pub fn repay<E, F>(&mut self, amount: u64, transfer: F) -> Result<(u64, u64), MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let debt_shares = self.position.debt_shares();
        let total_due = shares_to_amount(
            debt_shares,
            self.market.total_borrow_assets(),
            self.market.total_borrow_shares(),
        )
        .ok_or(MathError::Arithmetic)?;
        let repay_amount = amount.min(total_due);
        let shares_to_burn = if repay_amount == total_due {
            debt_shares
        } else {
            amount_to_shares_burned(
                repay_amount,
                self.market.total_borrow_assets(),
                self.market.total_borrow_shares(),
                debt_shares,
            )
            .ok_or(MathError::Arithmetic)?
        };
        *self.position.debt_shares_mut() = debt_shares
            .checked_sub(shares_to_burn)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_shares_mut() = self
            .market
            .total_borrow_shares()
            .checked_sub(shares_to_burn)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_assets_mut() = self
            .market
            .total_borrow_assets()
            .checked_sub(repay_amount)
            .ok_or(MathError::Arithmetic)?;
        transfer(repay_amount).map_err(MathError::Transfer)?;
        Ok((repay_amount, shares_to_burn))
    }

    /// Settle a matured rate hedge: close the borrower's floating debt
    /// (`initial_shares`) and re-borrow the fixed `borrow_amount`, paying the
    /// excess floating cost from the provider's collateral and the `upfront_fee`
    /// to the provider.
    ///
    /// **Solvency invariant (intentional):** the re-borrow is NOT gated by an LTV
    /// / oracle check. Settlement is a matured obligation — it must always
    /// complete so the provider's locked collateral and fee are released;
    /// blocking it on LTV would let a borrower whose collateral fell trap the
    /// counterparty's funds. If the settled position ends up above max LTV it
    /// becomes a liquidation target, which is the correct remedy — not a gate
    /// here. This is why the impl block carries no `O: Oracle` bound.
    pub fn settle_hedge<E, F1, F2>(
        &mut self,
        initial_shares: u64,
        borrow_amount: u64,
        upfront_fee: u64,
        excess_fn: F1,
        fee_fn: F2,
    ) -> Result<(u64, u64), MathError<E>>
    where
        F1: FnOnce(u64) -> Result<(), E>,
        F2: FnOnce(u64) -> Result<(), E>,
    {
        let current_value = shares_to_amount(
            initial_shares,
            self.market.total_borrow_assets(),
            self.market.total_borrow_shares(),
        )
        .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_shares_mut() = self
            .market
            .total_borrow_shares()
            .checked_sub(initial_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_assets_mut() = self
            .market
            .total_borrow_assets()
            .checked_sub(current_value)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_supply_assets_mut() = self
            .market
            .total_supply_assets()
            .saturating_sub(upfront_fee);
        let new_shares = amount_to_shares(
            borrow_amount,
            self.market.total_borrow_assets(),
            self.market.total_borrow_shares(),
        )
        .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_shares_mut() = self
            .market
            .total_borrow_shares()
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_assets_mut() = self
            .market
            .total_borrow_assets()
            .checked_add(borrow_amount)
            .ok_or(MathError::Arithmetic)?;
        let old_debt = self.position.debt_shares();
        *self.position.debt_shares_mut() = old_debt
            .checked_sub(initial_shares)
            .ok_or(MathError::Arithmetic)?
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        let fixed_total = borrow_amount
            .checked_add(upfront_fee)
            .ok_or(MathError::Arithmetic)?;
        let excess = current_value.saturating_sub(fixed_total);
        excess_fn(excess).map_err(MathError::Transfer)?;
        fee_fn(upfront_fee).map_err(MathError::Transfer)?;
        Ok((current_value, new_shares))
    }
}

// ── Methods available after accrual, requiring oracle ─────────────────────────

impl<M: Market, I, P, O: Oracle, S> Core<M, I, P, O, S> {
    pub fn oracle_price(&self) -> u64 {
        self.oracle.price()
    }
}

impl<M: Market, I, P: Position, O: Oracle> Core<M, I, P, O, Accrued> {
    pub fn borrow<E, F>(&mut self, amount: u64, transfer: F) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let oracle_price = self.oracle.price();
        let max_borrowable =
            self.max_borrow_capacity(self.position.collateral_deposited(), oracle_price);
        let current_debt = self.current_debt_amount().ok_or(MathError::Arithmetic)?;
        if amount
            > max_borrowable
                .checked_sub(current_debt)
                .ok_or(MathError::Undercollateralized)?
        {
            return Err(MathError::Undercollateralized);
        }
        let new_shares = amount_to_shares(
            amount,
            self.market.total_borrow_assets(),
            self.market.total_borrow_shares(),
        )
        .ok_or(MathError::Arithmetic)?;
        if new_shares == 0 {
            return Err(MathError::AmountTooSmall);
        }
        *self.position.debt_shares_mut() = self
            .position
            .debt_shares()
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_shares_mut() = self
            .market
            .total_borrow_shares()
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_assets_mut() = self
            .market
            .total_borrow_assets()
            .checked_add(amount)
            .ok_or(MathError::Arithmetic)?;
        transfer(amount).map_err(MathError::Transfer)?;
        Ok(new_shares)
    }

    pub fn borrow_with_fee<E, F>(
        &mut self,
        amount: u64,
        total_debt_amount: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let oracle_price = self.oracle.price();
        let max_borrowable =
            self.max_borrow_capacity(self.position.collateral_deposited(), oracle_price);
        let current_debt = self.current_debt_amount().ok_or(MathError::Arithmetic)?;
        // The position owes `total_debt_amount` (principal + fee), so the LTV gate
        // must be checked against the full recorded debt — not the pre-fee
        // `amount` the borrower receives — or the fee escapes collateralization.
        if total_debt_amount
            > max_borrowable
                .checked_sub(current_debt)
                .ok_or(MathError::Undercollateralized)?
        {
            return Err(MathError::Undercollateralized);
        }
        let new_shares = amount_to_shares(
            total_debt_amount,
            self.market.total_borrow_assets(),
            self.market.total_borrow_shares(),
        )
        .ok_or(MathError::Arithmetic)?;
        if new_shares == 0 {
            return Err(MathError::AmountTooSmall);
        }
        *self.position.debt_shares_mut() = self
            .position
            .debt_shares()
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_shares_mut() = self
            .market
            .total_borrow_shares()
            .checked_add(new_shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_borrow_assets_mut() = self
            .market
            .total_borrow_assets()
            .checked_add(total_debt_amount)
            .ok_or(MathError::Arithmetic)?;
        transfer(amount).map_err(MathError::Transfer)?;
        Ok(new_shares)
    }

    pub fn withdraw_collateral<E, F>(
        &mut self,
        amount: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let oracle_price = self.oracle.price();
        let remaining = self
            .position
            .collateral_deposited()
            .checked_sub(amount)
            .ok_or(MathError::InsufficientBalance)?;
        let max_borrowable = self.max_borrow_capacity(remaining, oracle_price);
        let current_debt = self.current_debt_amount().ok_or(MathError::Arithmetic)?;
        if current_debt > max_borrowable {
            return Err(MathError::Undercollateralized);
        }
        *self.position.collateral_deposited_mut() = remaining;
        transfer(amount).map_err(MathError::Transfer)?;
        Ok(remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct TestMarket {
        supply_a: u64,
        supply_s: u64,
        borrow_a: u64,
        borrow_s: u64,
        last_update: i64,
        aiq: u64,
        ltv: u8,
        fee: u64,
        fee_shares: u64,
    }
    impl Default for TestMarket {
        fn default() -> Self {
            TestMarket {
                supply_a: 0, supply_s: 0, borrow_a: 0, borrow_s: 0,
                last_update: 0, aiq: 0, ltv: 0, fee: 0, fee_shares: 0,
            }
        }
    }
    impl Market for TestMarket {
        fn total_supply_assets(&self) -> u64 { self.supply_a }
        fn total_supply_shares(&self) -> u64 { self.supply_s }
        fn total_borrow_assets(&self) -> u64 { self.borrow_a }
        fn total_borrow_shares(&self) -> u64 { self.borrow_s }
        fn last_update(&self) -> i64 { self.last_update }
        fn fee(&self) -> u64 { self.fee }
        fn accrued_fee_shares(&self) -> u64 { self.fee_shares }
        fn assets_in_queue(&self) -> u64 { self.aiq }
        fn ltv_percent(&self) -> u8 { self.ltv }
        fn total_supply_assets_mut(&mut self) -> &mut u64 { &mut self.supply_a }
        fn total_supply_shares_mut(&mut self) -> &mut u64 { &mut self.supply_s }
        fn accrued_fee_shares_mut(&mut self) -> &mut u64 { &mut self.fee_shares }
        fn assets_in_queue_mut(&mut self) -> &mut u64 { &mut self.aiq }
        fn total_borrow_assets_mut(&mut self) -> &mut u64 { &mut self.borrow_a }
        fn total_borrow_shares_mut(&mut self) -> &mut u64 { &mut self.borrow_s }
        fn last_update_mut(&mut self) -> &mut i64 { &mut self.last_update }
    }

    #[derive(Clone, Copy)]
    struct TestPosition { collateral: u64, debt_shares: u64 }
    impl Position for TestPosition {
        fn collateral_deposited(&self) -> u64 { self.collateral }
        fn debt_shares(&self) -> u64 { self.debt_shares }
        fn collateral_deposited_mut(&mut self) -> &mut u64 { &mut self.collateral }
        fn debt_shares_mut(&mut self) -> &mut u64 { &mut self.debt_shares }
    }

    #[derive(Clone, Copy)]
    struct TestIrm { rate_bps: u32, current_ts: i64 }
    impl IrmRate for TestIrm {
        fn rate_bps(&self) -> u32 { self.rate_bps }
        fn current_ts(&self) -> i64 { self.current_ts }
    }

    #[derive(Clone, Copy)]
    struct TestOracle { price: u64 }
    impl Oracle for TestOracle {
        fn price(&self) -> u64 { self.price }
    }

    /// Replays the exact on-chain full-repay sequence against the live devnet
    /// state that reported `MathOverflow`: pool 2a2xed at util≈24.75%
    /// (borrow_a=2_475_005, borrow_s=2_474_904), position debt_shares=2_474_904,
    /// IRM rate=154 bps, 50 s elapsed. Must NOT overflow — proving the reported
    /// error came from a stale deployed binary, not current source.
    #[test]
    fn full_repay_on_reported_devnet_state_does_not_overflow() {
        let market = TestMarket {
            supply_a: 10_000_000,
            supply_s: 10_000_000,
            borrow_a: 2_475_005,
            borrow_s: 2_474_904,
            last_update: 1_784_229_466,
            aiq: 0,
            ltv: 97,
            fee: 0,
            fee_shares: 0,
        };
        let position = TestPosition { collateral: 1_000_000_000, debt_shares: 2_474_904 };
        let irm = TestIrm { rate_bps: 154, current_ts: 1_784_229_516 };

        let mut core = Core::new(market)
            .with_irm(irm)
            .with_position(position)
            .accrue_interest()
            .expect("accrue_interest must not overflow");

        // Full repay: on-chain caller passes u64::MAX; Core caps at total_due.
        let (repaid, burned) = match core.repay(u64::MAX, |_| Ok::<(), ()>(())) {
            Ok(v) => v,
            Err(_) => panic!("repay must not overflow"),
        };

        assert_eq!(burned, 2_474_904, "burns entire debt");
        assert_eq!(core.position.debt_shares(), 0, "position cleared");
        assert_eq!(core.market.total_borrow_shares(), 0, "pool borrow shares cleared");
        // Interest accrued over 50 s at 154 bps on 2_475_005 rounds up to 1 unit.
        assert_eq!(repaid, 2_475_006);
        assert_eq!(core.market.total_borrow_assets(), 0, "pool borrow assets cleared");
    }

    /// H1 regression: accruing interest must credit the supply side so lenders
    /// earn yield. 100% APR for one year on 1_000_000 borrowed accrues 1_000_000
    /// interest, added to BOTH borrow and supply assets (books stay balanced),
    /// doubling the LP share price.
    #[test]
    fn accrue_interest_credits_lenders_supply_side() {
        let year = crate::SECONDS_PER_YEAR as i64;
        let market = TestMarket {
            supply_a: 1_000_000,
            supply_s: 1_000_000,
            borrow_a: 1_000_000,
            borrow_s: 1_000_000,
            last_update: 1_000,
            aiq: 0,
            ltv: 75,
            fee: 0,
            fee_shares: 0,
        };
        let irm = TestIrm { rate_bps: 10_000, current_ts: 1_000 + year };

        let core = Core::new(market)
            .with_irm(irm)
            .accrue_interest()
            .expect("accrue must not overflow");

        assert_eq!(core.market.total_borrow_assets(), 2_000_000);
        assert_eq!(
            core.market.total_supply_assets(),
            2_000_000,
            "lenders must be credited the accrued interest"
        );
        // 1_000_000 LP shares now redeem for 2_000_000 assets — yield accrued.
        assert_eq!(core.calc_lend_for_shares(1_000_000), Some(2_000_000));
    }

    /// Protocol fee: with `fee` = 1_000 bps (10%), a 1_000_000 interest accrual
    /// gives lenders 90% and mints the protocol fee shares worth the other 10%.
    /// Books stay balanced: supply_assets holds the full interest.
    #[test]
    fn accrue_interest_mints_protocol_fee_shares() {
        let year = crate::SECONDS_PER_YEAR as i64;
        let market = TestMarket {
            supply_a: 1_000_000,
            supply_s: 1_000_000,
            borrow_a: 1_000_000,
            borrow_s: 1_000_000,
            last_update: 1_000,
            fee: 1_000, // 10% of interest to the protocol
            ..Default::default()
        };
        let irm = TestIrm { rate_bps: 10_000, current_ts: 1_000 + year };

        let core = Core::new(market)
            .with_irm(irm)
            .accrue_interest()
            .expect("accrue must not overflow");

        // Full interest (1_000_000) credited to supply; books balanced.
        assert_eq!(core.market.total_supply_assets(), 2_000_000);
        // feeAmount = 100_000; feeShares = 100_000 × 1_000_000 / (2_000_000 −
        // 100_000) = 100_000_000_000 / 1_900_000 = 52_631.
        let fee_shares = core.market.accrued_fee_shares();
        assert_eq!(fee_shares, 52_631);
        assert_eq!(core.market.total_supply_shares(), 1_000_000 + fee_shares);
        // The protocol's shares are worth ~fee_amount (10% of interest). Rounding
        // (fee shares floored, valuation floored) makes it under-collect by a few
        // units — the protocol-safe direction, never over.
        let protocol_value = core.calc_lend_for_shares(fee_shares).unwrap();
        assert!(
            protocol_value <= 100_000 && 100_000 - protocol_value <= 3,
            "protocol ≈ 10% of interest, got {protocol_value}"
        );
        // Lenders' original shares keep the remaining ~90% (plus principal), and
        // the two claims together never exceed total supply assets.
        let lender_value = core.calc_lend_for_shares(1_000_000).unwrap();
        assert!(lender_value.abs_diff(1_900_000) <= 2, "lenders ≈ 90% + principal");
        assert!(protocol_value + lender_value <= 2_000_000, "no over-issuance");
    }

    /// H2 regression: `borrow_with_fee` must gate LTV on the full recorded debt
    /// (`total_debt_amount` = principal + fee), not the pre-fee principal, or the
    /// fee lets a position exceed max LTV. Capacity here is
    /// 1_000_000 × 1.0 × 75% = 750_000.
    #[test]
    fn borrow_with_fee_gates_ltv_on_total_debt() {
        let market = TestMarket {
            supply_a: 10_000_000,
            supply_s: 10_000_000,
            borrow_a: 0,
            borrow_s: 0,
            last_update: 1_000,
            aiq: 0,
            ltv: 75,
            fee: 0,
            fee_shares: 0,
        };
        let position = TestPosition { collateral: 1_000_000, debt_shares: 0 };
        let irm = TestIrm { rate_bps: 0, current_ts: 1_000 };
        let oracle = TestOracle { price: PRICE_SCALE as u64 };
        let fresh = || {
            Core::new(market)
                .with_position(position)
                .with_oracle(oracle)
                .with_irm(irm)
                .accrue_interest()
                .expect("accrue must not overflow")
        };

        // Principal 750_000 alone fits capacity, but principal + 1 fee exceeds it
        // and must be rejected — the fee is part of the debt.
        let mut over = fresh();
        assert!(
            matches!(
                over.borrow_with_fee(750_000, 750_001, |_| Ok::<(), ()>(())),
                Err(MathError::Undercollateralized)
            ),
            "fee pushing debt over capacity must be Undercollateralized"
        );

        // Total debt exactly at capacity (principal + fee = 750_000) succeeds.
        let mut at = fresh();
        assert!(
            at.borrow_with_fee(749_999, 750_000, |_| Ok::<(), ()>(())).is_ok(),
            "total debt at exact capacity must succeed"
        );
    }

    /// `max_borrow_capacity` saturates rather than erroring: at `u64::MAX`
    /// collateral and price it clamps to `u64::MAX` instead of overflowing, so a
    /// large-but-valid position is never bricked by the capacity gate.
    #[test]
    fn max_borrow_capacity_saturates_at_u64_max() {
        let market = TestMarket {
            supply_a: 0,
            supply_s: 0,
            borrow_a: 0,
            borrow_s: 0,
            last_update: 0,
            aiq: 0,
            ltv: 75,
            fee: 0,
            fee_shares: 0,
        };
        let position = TestPosition { collateral: 0, debt_shares: 0 };
        let core = Core::new(market).with_position(position);
        assert_eq!(
            core.max_borrow_capacity(u64::MAX, u64::MAX),
            u64::MAX,
            "overflowing capacity must clamp to u64::MAX"
        );
        // A normal case still computes exactly: 1_000_000 × 1.0 × 75% = 750_000.
        assert_eq!(core.max_borrow_capacity(1_000_000, PRICE_SCALE as u64), 750_000);
    }

    /// Borrow capacity boundary via the real `Core::borrow` op (the same gate the
    /// program enforces): capacity =
    /// collateral × oracle_price / PRICE_SCALE × ltv / 100.
    /// Here 1_000_000 collateral × 1.0 price × 75% = 750_000 lend units.
    /// Borrowing exactly the capacity succeeds; one unit more is rejected
    /// `Undercollateralized`.
    #[test]
    fn borrow_at_capacity_succeeds_one_over_fails() {
        let market = TestMarket {
            supply_a: 10_000_000,
            supply_s: 10_000_000,
            borrow_a: 0,
            borrow_s: 0,
            last_update: 1_784_229_466,
            aiq: 0,
            ltv: 75,
            fee: 0,
            fee_shares: 0,
        };
        let position = TestPosition { collateral: 1_000_000, debt_shares: 0 };
        let irm = TestIrm { rate_bps: 154, current_ts: 1_784_229_516 };
        let oracle = TestOracle { price: PRICE_SCALE as u64 };

        let fresh = || {
            Core::new(market)
                .with_position(position)
                .with_oracle(oracle)
                .with_irm(irm)
                .accrue_interest()
                .expect("accrue must not overflow")
        };

        const CAPACITY: u64 = 750_000;

        // Exactly at capacity: succeeds, minting the first-borrow 1:1 shares.
        let mut at_cap = fresh();
        match at_cap.borrow(CAPACITY, |_| Ok::<(), ()>(())) {
            Ok(shares) => assert_eq!(shares, CAPACITY, "first borrow mints 1:1 shares"),
            Err(_) => panic!("borrow at exact capacity must succeed"),
        }

        // One unit over capacity: rejected by the LTV gate.
        let mut over_cap = fresh();
        assert!(
            matches!(
                over_cap.borrow(CAPACITY + 1, |_| Ok::<(), ()>(())),
                Err(MathError::Undercollateralized)
            ),
            "borrowing capacity + 1 must be Undercollateralized"
        );
    }
}

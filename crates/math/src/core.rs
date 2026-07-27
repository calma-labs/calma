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
#[path = "core_tests.rs"]
mod tests;

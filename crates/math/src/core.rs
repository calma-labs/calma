use std::marker::PhantomData;

use crate::{
    amount_to_shares, amount_to_shares_burned, compute_interest, shares_to_amount, IrmRate, Market,
    MathError, Oracle, Position, PRICE_SCALE,
};

/// Flash loan fee in basis points (9 bps = 0.09%). Single source of truth shared
/// by the on-chain program and the client bindings.
pub const FLASH_LOAN_FEE_BPS: u64 = 9;

/// Flash-loan fee owed on `amount` at the protocol flash-fee rate.
pub fn flash_fee(amount: u64) -> Option<u64> {
    u64::try_from(
        (amount as u128)
            .checked_mul(FLASH_LOAN_FEE_BPS as u128)?
            .checked_div(10_000)?,
    )
    .ok()
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
        let clamped = lend.min(self.market.total_supply_assets());
        *self.market.total_supply_shares_mut() = self
            .market
            .total_supply_shares()
            .checked_sub(shares)
            .ok_or(MathError::Arithmetic)?;
        *self.market.total_supply_assets_mut() = self
            .market
            .total_supply_assets()
            .checked_sub(clamped)
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

    fn max_borrow_capacity(&self, collateral: u64, oracle_price: u64) -> Option<u64> {
        u64::try_from(
            (collateral as u128)
                .checked_mul(oracle_price as u128)?
                .checked_div(PRICE_SCALE)?
                .checked_mul(self.market.ltv_percent() as u128)?
                .checked_div(100)?,
        )
        .ok()
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
        let max_borrowable = self
            .max_borrow_capacity(self.position.collateral_deposited(), oracle_price)
            .ok_or(MathError::Arithmetic)?;
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
        let max_borrowable = self
            .max_borrow_capacity(self.position.collateral_deposited(), oracle_price)
            .ok_or(MathError::Arithmetic)?;
        let current_debt = self.current_debt_amount().ok_or(MathError::Arithmetic)?;
        if amount
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
        let max_borrowable = self
            .max_borrow_capacity(remaining, oracle_price)
            .ok_or(MathError::Arithmetic)?;
        let current_debt = self.current_debt_amount().ok_or(MathError::Arithmetic)?;
        if current_debt > max_borrowable {
            return Err(MathError::Undercollateralized);
        }
        *self.position.collateral_deposited_mut() = remaining;
        transfer(amount).map_err(MathError::Transfer)?;
        Ok(remaining)
    }
}

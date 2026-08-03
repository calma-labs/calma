use std::marker::PhantomData;

use crate::{
    amount_to_shares, amount_to_shares_burned, compute_interest, shares_to_amount, IrmRate, Market,
    MathError, Oracle, Position,
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

/// Minimum repayment owed on a flash loan of `amount`: principal plus fee.
///
/// Shared with the on-chain instruction-sysvar scan, which must know the figure
/// *before* `Core::flash_borrow` runs, so the formula is not restated there.
pub fn flash_min_repay(amount: u64) -> Option<u64> {
    amount.checked_add(flash_fee(amount)?)
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

    /// Open a flash loan of `amount` against a vault holding `vault_balance`.
    ///
    /// Takes the pairing lock and hands the tokens out; returns the minimum
    /// repayment (`amount` + fee) the caller must see returned before the
    /// transaction ends.
    ///
    /// `total_supply_assets` is deliberately **not** debited. A flash loan does
    /// not reduce what lenders own — the tokens are contractually back before
    /// the transaction ends. Debiting it mid-transaction moved the LP share
    /// price, letting a caller mint LP at the depressed price between borrow and
    /// repay and redeem at the restored one (and, when the loan emptied the
    /// supply side, mint 1:1 against the whole outstanding share supply). Only
    /// the fee is credited, on repay.
    pub fn flash_borrow<E, F>(
        &mut self,
        amount: u64,
        vault_balance: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        if amount == 0 {
            return Err(MathError::InvalidAmount);
        }
        if self.market.flash_loan_outstanding() != 0 {
            return Err(MathError::FlashLoanOutstanding);
        }
        if amount > crate::borrowable_liquidity(vault_balance, self.market.assets_in_queue()) {
            return Err(MathError::InsufficientLiquidity);
        }
        let fee = crate::flash_fee(amount).ok_or(MathError::Arithmetic)?;
        let min_repay = amount.checked_add(fee).ok_or(MathError::Arithmetic)?;
        *self.market.flash_loan_outstanding_mut() = amount;
        transfer(amount).map_err(MathError::Transfer)?;
        Ok(min_repay)
    }

    /// Close the in-flight flash loan with a repayment of `amount`.
    ///
    /// Returns `(principal, fee)`. `amount` must be *exactly* principal plus
    /// fee — the figure [`crate::flash_min_repay`] returns for the borrow — and
    /// only the fee is credited to the supply side as lender yield. The lock is
    /// released only once the repayment has actually been made.
    ///
    /// # Why over-repayment is refused rather than kept
    ///
    /// This used to accept any `amount >= min_repay` and credit everything above
    /// the principal to `total_supply_assets`, treating the excess as a
    /// donation. That was an unbounded write into the supply side by a
    /// caller-chosen number, and it defeated the share-inflation guarantee
    /// stated on [`Self::deposit_lent`] — which rests on `total_supply_assets`
    /// only ever moving through this crate's own accounting, never through
    /// tokens someone decided to hand over.
    ///
    /// Concretely: seed a fresh pool with one base unit to hold the only share,
    /// flash-borrow that unit and repay it with a large surplus, and the share
    /// price becomes the surplus. The next depositor's
    /// `amount × shares / assets` then floors to a single share however much
    /// they deposited, and the seeder redeems a cut of it. The `lp == 0` guard
    /// bounded that theft at just under half the victim's deposit; it did not
    /// prevent it.
    ///
    /// Refusing the surplus outright is preferred over silently capping the
    /// credit at the fee. Capping would leave the excess tokens sitting in the
    /// vault outside the accounting — permanently unclaimable by anyone, and a
    /// standing drift between the vault balance and the books. `min_repay` is a
    /// pure function of the borrowed amount, exported as
    /// [`crate::flash_min_repay`] and fixed at the moment the borrow is built,
    /// so there is no race that would make hitting it exactly unreliable.
    pub fn flash_repay<E, F>(&mut self, amount: u64, transfer: F) -> Result<(u64, u64), MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        let principal = self.market.flash_loan_outstanding();
        if principal == 0 {
            return Err(MathError::NoFlashLoan);
        }
        let fee = crate::flash_fee(principal).ok_or(MathError::Arithmetic)?;
        let min_repay = principal.checked_add(fee).ok_or(MathError::Arithmetic)?;
        if amount < min_repay {
            return Err(MathError::FlashLoanUnderRepaid);
        }
        if amount > min_repay {
            return Err(MathError::FlashLoanOverRepaid);
        }
        transfer(amount).map_err(MathError::Transfer)?;
        // Exactly the fee, by construction: `amount == principal + fee`. The
        // principal itself is not credited — it never left the supply side, as
        // `flash_borrow` deliberately does not debit it.
        *self.market.total_supply_assets_mut() = self
            .market
            .total_supply_assets()
            .checked_add(fee)
            .ok_or(MathError::Arithmetic)?;
        *self.market.flash_loan_outstanding_mut() = 0;
        Ok((principal, fee))
    }

    /// `true` while a flash loan is in flight, i.e. while the vault is
    /// temporarily short and share-price-sensitive operations must not run.
    pub fn flash_loan_in_progress(&self) -> bool {
        self.market.flash_loan_outstanding() != 0
    }

    /// Settle one previously queued withdrawal of `amount` lend tokens.
    ///
    /// Counterpart to [`Self::withdraw_lent_queued`], which already burned the
    /// shares and moved `amount` out of `total_supply_assets` into
    /// `assets_in_queue`. Here that reservation is simply released as the tokens
    /// go out, so the two operations together are exactly equivalent to
    /// [`Self::withdraw_lent_immediate`].
    ///
    /// Unlike the other LP operations this is **not** share-price sensitive and
    /// so needs no `Accrued` market: the shares are already gone and `amount`
    /// was fixed when the entry was enqueued. Any interest accrued since then
    /// stays with the lenders still in the pool.
    pub fn process_queued_withdrawal<E, F>(
        &mut self,
        amount: u64,
        transfer: F,
    ) -> Result<(), MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        *self.market.assets_in_queue_mut() = self
            .market
            .assets_in_queue()
            .checked_sub(amount)
            .ok_or(MathError::Arithmetic)?;
        transfer(amount).map_err(MathError::Transfer)
    }
}

// ── LP entry / exit — only valid on an accrued market ─────────────────────────
//
// These move value between the lender and the pool at the current share price,
// so they MUST run against a market whose interest is up to date. Living behind
// the `Accrued` typestate makes a missing `accrue_interest()` a compile error
// rather than a silent mispricing: a caller who skips accrual lets a
// just-in-time depositor mint shares at the stale (low) price, poke accrual, and
// redeem a cut of interest that accrued before they arrived.
impl<M: Market, I, P, O> Core<M, I, P, O, Accrued> {
    /// Share-inflation posture — two independent guards, both load-bearing.
    ///
    /// 1. **No donation channel.** `total_supply_assets` is internally accounted
    ///    (updated only by this crate), never read from the vault's token
    ///    balance, so a direct transfer into the vault cannot move the share
    ///    price. That alone was once considered sufficient and was not:
    ///    [`Self::flash_repay`] credited a caller-chosen surplus straight into
    ///    the same field, which is a donation channel by another name. It now
    ///    credits exactly the fee. Any future write to `total_supply_assets`
    ///    must be bounded by protocol arithmetic, not by a caller's argument.
    ///
    /// 2. **No one-share pool.** Seeding is refused below
    ///    [`crate::MIN_SEED_LIQUIDITY`], so the share price can never be
    ///    anchored to a single unit. This is what keeps guard 1 from being the
    ///    only thing standing between a fresh pool and a rounding attack.
    ///
    /// The `lp == 0` guard below (and the `new_shares == 0` guard in `borrow`)
    /// reject zero-share griefing, and bound — but do not prevent — the loss if
    /// a share price is manipulated anyway. No virtual-shares offset is applied;
    /// deposits round down and the first depositor seeds 1:1 above the floor.
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
        if amount == 0 {
            return Err(MathError::InvalidAmount);
        }
        let lp = if self.market.total_supply_shares() == 0 || self.market.total_supply_assets() == 0
        {
            // Seeding the supply side: shares are minted 1:1, so this deposit
            // alone sets the share price every later depositor is quoted
            // against. A floor here is what stops that price being anchored to a
            // handful of units — see `MIN_SEED_LIQUIDITY`.
            if amount < crate::MIN_SEED_LIQUIDITY {
                return Err(MathError::AmountTooSmall);
            }
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

    /// Queue a withdrawal: burn the shares now, pay the tokens later.
    ///
    /// Both sides of the ratio must move together. Burning the shares while
    /// leaving their assets in `total_supply_assets` inflates the share price for
    /// everyone still in the pool, so each subsequent queued exit is quoted
    /// against a denominator that keeps shrinking against a fixed numerator —
    /// three equal lenders queueing in turn were quoted 1x, 1.5x and 3x their
    /// deposit. The queue then promises more than the pool holds and whoever is
    /// last in line can never be paid.
    ///
    /// Moving the assets into `assets_in_queue` at the same time keeps the share
    /// price flat for remaining lenders and makes the claim exactly what an
    /// immediate withdrawal would have paid.
    pub fn withdraw_lent_queued(&mut self, shares: u64) -> Option<u64> {
        let lend = self.calc_lend_for_shares(shares)?;
        *self.market.total_supply_shares_mut() =
            self.market.total_supply_shares().checked_sub(shares)?;
        *self.market.total_supply_assets_mut() =
            self.market.total_supply_assets().checked_sub(lend)?;
        *self.market.assets_in_queue_mut() = self.market.assets_in_queue().checked_add(lend)?;
        Some(lend)
    }

}

impl<M: Market, I, P: Position, O, S> Core<M, I, P, O, S> {
    pub fn deposit_collateral<E, F>(&mut self, amount: u64, transfer: F) -> Result<(), MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        if amount == 0 {
            return Err(MathError::InvalidAmount);
        }
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
    /// Returns `None` rather than clamping when the capacity exceeds `u64`.
    /// Clamping to `u64::MAX` was the **unsafe** direction: this figure is the
    /// ceiling every LTV gate compares against, so an overflow that saturated
    /// high silently granted unlimited borrow headroom and let
    /// `withdraw_collateral` release collateral it should have held. Refusing an
    /// unrepresentable position is the conservative failure.
    ///
    /// The collateral→lend conversion is
    /// [`collateral_value_in_lend`](crate::collateral_value_in_lend), shared with
    /// the client's LTV and health-factor figures so the warning a user sees and
    /// the gate the chain enforces cannot drift apart. It cannot overflow `u128`
    /// (both operands ≤ `u64::MAX`), and neither can the `ltv_percent` scaling
    /// that follows (≤ `u64::MAX²/PRICE_SCALE × 99`).
    pub fn max_borrow_capacity(&self, collateral: u64, oracle_price: u64) -> Option<u64> {
        let capacity =
            crate::collateral_value_in_lend(collateral, oracle_price)
                * (self.market.ltv_percent() as u128)
                / 100;
        u64::try_from(capacity).ok()
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
}

// ── Methods available after accrual, requiring oracle ─────────────────────────

impl<M: Market, I, P, O: Oracle, S> Core<M, I, P, O, S> {
    pub fn oracle_price(&self) -> u64 {
        self.oracle.price()
    }
}

impl<M: Market, I, P: Position, O: Oracle> Core<M, I, P, O, Accrued> {
    pub fn borrow<E, F>(
        &mut self,
        amount: u64,
        vault_balance: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        if amount == 0 {
            return Err(MathError::InvalidAmount);
        }
        if amount > crate::borrowable_liquidity(vault_balance, self.market.assets_in_queue()) {
            return Err(MathError::InsufficientLiquidity);
        }
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

    pub fn withdraw_collateral<E, F>(
        &mut self,
        amount: u64,
        transfer: F,
    ) -> Result<u64, MathError<E>>
    where
        F: FnOnce(u64) -> Result<(), E>,
    {
        if amount == 0 {
            return Err(MathError::InvalidAmount);
        }
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

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;

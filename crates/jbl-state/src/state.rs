use crate::{withdrawal_queue::WithdrawalQueue, UtilizationFeeConfig};
use anchor_lang::prelude::*;
use jbl_math::compute_interest;

/// Unified lending pool account stored as zero-copy.
///
/// Manages three token mints:
///   - `collateral_mint`: tokens users deposit as collateral to enable borrowing
///   - `lend_mint`:       tokens lenders deposit (earning LP) and borrowers receive
///   - `lp_mint`:         issued 1:1 to lend-side depositors; redeemable for lend tokens
///
/// Field ordering eliminates implicit repr(C) padding:
///   offsets 0-127   : four Pubkeys  (4 × 32 = 128 bytes, align 1)
///   offsets 128-175 : six u64/i64  (6 × 8 = 48)
///   offsets 176-207 : fee_config   (4 × u64 = 32)
///   offsets 208-209 : ltv_percent, lp_mint_bump  (2 × u8)
///   offsets 210-215 : _pad [u8; 6]  (align withdrawal_queue to 8)
///   offsets 216-... : WithdrawalQueue  (1024 entries × 40 bytes = 40 960 + 8 header)
#[account(zero_copy)]
pub struct Pool {
    pub authority: Pubkey,
    /// Mint of the token deposited as collateral (tracked raw, no LP issued).
    pub collateral_mint: Pubkey,
    /// Mint of the token lenders deposit and borrowers receive.
    pub lend_mint: Pubkey,
    /// LP token mint issued to lend-side depositors.
    pub lp_mint: Pubkey,
    /// Raw sum of collateral tokens deposited across all positions.
    pub total_collateral_deposited: u64,
    /// Sum of lend tokens currently deposited (basis for LP ratio and utilisation).
    pub total_lend_deposited: u64,
    pub total_borrowed: u64,
    pub total_debt_shares: u64,
    pub last_accrual_ts: i64,
    /// Total LP tokens outstanding for the lend side.
    pub total_lp_issued: u64,
    pub fee_config: UtilizationFeeConfig,
    pub ltv_percent: u8,
    pub lp_mint_bump: u8,
    _pad: [u8; 6], // explicit padding — no implicit/uninitialised bytes
    /// Queue of pending lend-token withdrawals (LP burned at `leave` time).
    pub withdrawal_queue: WithdrawalQueue,
}

impl Pool {
    pub fn calculate_utilization(&self) -> u64 {
        if self.total_lend_deposited == 0 {
            0
        } else {
            (self.total_borrowed as u128)
                .checked_mul(10_000)
                .unwrap_or(0)
                .checked_div(self.total_lend_deposited as u128)
                .unwrap_or(0) as u64
        }
    }

    /// Accrue interest into `total_borrowed` based on elapsed time since last
    /// accrual, then update `last_accrual_ts` to `current_ts`.
    pub fn accrue_interest(&mut self, current_ts: i64) -> Result<()> {
        let elapsed = (current_ts.saturating_sub(self.last_accrual_ts)).max(0) as u64;
        if elapsed == 0 {
            return Ok(());
        }

        let utilization = self.calculate_utilization();
        let fee_bps = self.fee_config.get_fee_bps(utilization);

        let interest = compute_interest(self.total_borrowed, fee_bps, elapsed)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        self.total_borrowed = self
            .total_borrowed
            .checked_add(interest)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        self.last_accrual_ts = current_ts;
        Ok(())
    }
}

/// An offer to hedge interest-rate risk at a fixed rate.
///
/// The creator locks `collateral_deposited` tokens as collateral for the duration of any
/// accepted match. The PDA seeds encode the offer parameters so two distinct offers with
/// different rates or durations cannot alias each other.
///
/// Seeds: `["rate_hedge_offer", pool, authority, fixed_rate_le, min_duration_le, max_duration_le]`
///
/// Memory layout (repr C, no implicit padding):
///   offsets 0-31  : pool        (32 bytes)
///   offsets 32-63 : authority   (32 bytes)
///   offsets 64-71 : amount      (8)
///   offsets 72-79 : fixed_rate_bps (8)
///   offsets 80-87 : min_duration (8)
///   offsets 88-95 : max_duration (8)
///   offsets 96-103: collateral_deposited (8)
///   offsets 104-111: locked_tokens (8)
///   offset 112    : bump        (1)
///   offsets 113-119: _pad       (7)
#[account(zero_copy)]
pub struct RateHedgeOffer {
    /// The pool this offer is associated with.
    pub pool: Pubkey,
    /// The user who created the offer.
    pub authority: Pubkey,
    /// Notional amount of lend tokens covered by this offer.
    pub amount: u64,
    /// Fixed interest rate in basis points (e.g. 500 = 5.00 %).
    pub fixed_rate_bps: u64,
    /// Minimum acceptable duration for a match, in seconds.
    pub min_duration: u64,
    /// Maximum acceptable duration for a match, in seconds.
    pub max_duration: u64,
    /// Collateral tokens locked as security when the offer was created.
    pub collateral_deposited: u64,
    /// Running total of upfront fees owed to the offer creator across all active matches.
    /// At match settlement these tokens are transferred from the lend vault to the creator.
    pub locked_tokens: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

/// Records an active rate-hedge match between a borrower and an offer creator.
///
/// Created atomically with a hedged borrow. Lives until the crank settles it after
/// `start_ts + duration` has elapsed.
///
/// Seeds: `["rate_hedge_match", user_position]`
/// (one active match per user position at a time)
///
/// Memory layout (repr C, no implicit padding):
///   offsets 0-31  : offer       (32 bytes)
///   offsets 32-63 : user_position (32 bytes)
///   offsets 64-71 : amount      (8)
///   offsets 72-79 : upfront_fee (8)
///   offsets 80-87 : initial_debt_shares (8)
///   offsets 88-95 : start_ts    (8)
///   offsets 96-103: duration    (8)
///   offset 104    : bump        (1)
///   offsets 105-111: _pad       (7)
#[account(zero_copy)]
pub struct RateHedgeMatch {
    /// The offer backing this match.
    pub offer: Pubkey,
    /// The borrower's position account.
    pub user_position: Pubkey,
    /// Original borrow amount (lend tokens received by the borrower).
    /// This is the borrower's repayment cap — they will never owe more than this
    /// as a result of the hedge.
    pub amount: u64,
    /// Upfront fixed fee paid into the pool as additional debt shares.
    /// At settlement this is transferred from the lend vault to the offer creator.
    pub upfront_fee: u64,
    /// Debt shares issued at match creation, covering `amount + upfront_fee`.
    /// Used at settlement to compute how much the variable rate actually grew.
    pub initial_debt_shares: u64,
    /// Unix timestamp when the match was created.
    pub start_ts: i64,
    /// Agreed hedge duration in seconds.
    pub duration: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

/// Tracks a user's collateral deposit and any open borrow position.
/// Created on first collateral deposit; borrow fields populated when the user borrows.
///
/// Memory layout (repr C, no implicit padding):
///   offsets 0-31  : authority   (32 bytes)
///   offsets 32-63 : pool        (32 bytes)
///   offsets 64-71 : collateral_deposited (8)
///   offsets 72-79 : debt_shares (8)
///   offset 80     : bump        (1)
///   offsets 81-87 : _pad        (7)
#[account(zero_copy)]
pub struct UserPosition {
    /// The user who owns this position
    pub authority: Pubkey,
    /// The pool this position belongs to
    pub pool: Pubkey,
    /// Raw amount of collateral tokens deposited (no shares — tracked 1:1).
    pub collateral_deposited: u64,
    /// Debt shares held by this user (0 if no active borrow).
    /// Current debt = debt_shares * pool.total_borrowed / pool.total_debt_shares
    pub debt_shares: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

#[cfg(test)]
mod size_tests {
    use super::*;
    use std::mem::size_of;

    /// Space values hardcoded in instruction `init` constraints must match the
    /// actual struct size (discriminator excluded — Anchor adds 8 bytes on top).
    #[test]
    fn pool_size() {
        // 4 Pubkeys (128) + 6 u64/i64 (48) + UtilizationFeeConfig (32) +
        // ltv_percent + lp_mint_bump (2) + _pad (6) + WithdrawalQueue
        // Full value checked against POOL_SPACE constant in test utils (41 184).
        assert_eq!(size_of::<Pool>(), 41_184);
    }

    #[test]
    fn user_position_size() {
        // authority(32) + pool(32) + collateral_deposited(8) + debt_shares(8) + bump(1) + _pad(7) = 88
        assert_eq!(size_of::<UserPosition>(), 88);
    }

    #[test]
    fn rate_hedge_offer_size() {
        // pool(32) + authority(32) + amount(8) + fixed_rate_bps(8) +
        // min_duration(8) + max_duration(8) + collateral_deposited(8) +
        // locked_tokens(8) + bump(1) + _pad(7) = 120
        assert_eq!(size_of::<RateHedgeOffer>(), 120);
    }

    #[test]
    fn rate_hedge_match_size() {
        // offer(32) + user_position(32) + amount(8) + upfront_fee(8) +
        // initial_debt_shares(8) + start_ts(8) + duration(8) + bump(1) + _pad(7) = 112
        assert_eq!(size_of::<RateHedgeMatch>(), 112);
    }
}

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
///   offsets 176-271 : fee_config   (4 curves × 24 bytes = 96 bytes)
///   offsets 272-273 : ltv_percent, lp_mint_bump  (2 × u8)
///   offsets 274-279 : _pad [u8; 6]  (align withdrawal_queue to 8)
///   offsets 280-... : WithdrawalQueue  (1024 entries × 40 bytes = 40 960 + 8 header)
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

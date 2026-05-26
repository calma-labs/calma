use crate::{withdrawal_queue::WithdrawalQueue, UtilizationFeeConfig};
use anchor_lang::prelude::*;
use jbl_math::compute_interest;

/// Per-market accounting: supply, borrow, and fee state.
#[zero_copy]
pub struct Market {
    /// Sum of lend tokens currently deposited (basis for LP ratio and utilisation).
    pub total_supply_assets: u64,
    /// Total LP tokens outstanding for the lend side.
    pub total_supply_shares: u64,
    pub total_borrow_assets: u64,
    pub total_borrow_shares: u64,
    pub last_update: i64,
    pub fee: u64,
    pub assets_in_queue: u64,
}

/// Unified lending pool account stored as zero-copy.
///
/// Manages three token mints:
///   - `collateral_mint`: tokens users deposit as collateral to enable borrowing
///   - `lend_mint`:       tokens lenders deposit (earning LP) and borrowers receive
///   - `lp_mint`:         issued 1:1 to lend-side depositors; redeemable for lend tokens
///
/// Field ordering eliminates implicit repr(C) padding:
///   offsets 0-127   : four Pubkeys  (4 × 32 = 128 bytes, align 1)
///   offsets 128-135 : total_collateral_deposited (u64)
///   offsets 136-191 : market (Market, 7 × 8 = 56 bytes)
///   offsets 192-287 : fee_config   (4 curves × 24 bytes = 96 bytes)
///   offsets 288-289 : ltv_percent, lp_mint_bump  (2 × u8)
///   offsets 290-295 : _pad [u8; 6]  (align withdrawal_queue to 8)
///   offsets 296-... : WithdrawalQueue  (1024 entries × 40 bytes = 40 960 + 8 header)
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
    pub market: Market,
    pub fee_config: UtilizationFeeConfig,
    pub ltv_percent: u8,
    pub lp_mint_bump: u8,
    _pad: [u8; 6], // explicit padding — no implicit/uninitialised bytes
    /// Queue of pending lend-token withdrawals (LP burned at `leave` time).
    pub withdrawal_queue: WithdrawalQueue,
}

impl Pool {
    pub fn calculate_utilization(&self) -> u64 {
        if self.market.total_supply_assets == 0 {
            0
        } else {
            (self.market.total_borrow_assets as u128)
                .checked_mul(10_000)
                .unwrap_or(0)
                .checked_div(self.market.total_supply_assets as u128)
                .unwrap_or(0) as u64
        }
    }

    /// Accrue interest into `total_borrow_assets` based on elapsed time since last
    /// accrual, then update `market.last_update` to `current_ts`.
    pub fn accrue_interest(&mut self, current_ts: i64) -> Result<()> {
        let elapsed = (current_ts.saturating_sub(self.market.last_update)).max(0) as u64;
        if elapsed == 0 {
            return Ok(());
        }

        let utilization = self.calculate_utilization();
        let fee_bps = self.fee_config.get_fee_bps(utilization);

        let interest = compute_interest(self.market.total_borrow_assets, fee_bps, elapsed)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        self.market.total_borrow_assets = self
            .market
            .total_borrow_assets
            .checked_add(interest)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        self.market.last_update = current_ts;
        Ok(())
    }
}

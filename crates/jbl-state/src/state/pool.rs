use crate::withdrawal_queue::WithdrawalQueue;
use anchor_lang::prelude::*;

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
    pub ltv_percent: u8,
    _pad: [u8; 7],
}

impl jbl_math::Market for Market {
    fn total_supply_assets(&self) -> u64 {
        self.total_supply_assets
    }
    fn total_supply_shares(&self) -> u64 {
        self.total_supply_shares
    }
    fn total_borrow_assets(&self) -> u64 {
        self.total_borrow_assets
    }
    fn total_borrow_shares(&self) -> u64 {
        self.total_borrow_shares
    }
    fn last_update(&self) -> i64 {
        self.last_update
    }
    fn fee(&self) -> u64 {
        self.fee
    }
    fn assets_in_queue(&self) -> u64 {
        self.assets_in_queue
    }
    fn ltv_percent(&self) -> u8 {
        self.ltv_percent
    }
    fn total_supply_assets_mut(&mut self) -> &mut u64 {
        &mut self.total_supply_assets
    }
    fn total_supply_shares_mut(&mut self) -> &mut u64 {
        &mut self.total_supply_shares
    }
    fn assets_in_queue_mut(&mut self) -> &mut u64 {
        &mut self.assets_in_queue
    }
    fn total_borrow_assets_mut(&mut self) -> &mut u64 {
        &mut self.total_borrow_assets
    }
    fn total_borrow_shares_mut(&mut self) -> &mut u64 {
        &mut self.total_borrow_shares
    }
    fn last_update_mut(&mut self) -> &mut i64 {
        &mut self.last_update
    }
}

/// Unified lending pool account stored as zero-copy.
///
/// Manages three token mints:
///   - `collateral_mint`: tokens users deposit as collateral to enable borrowing
///   - `lend_mint`:       tokens lenders deposit (earning LP) and borrowers receive
///   - `lp_mint`:         issued 1:1 to lend-side depositors; redeemable for lend tokens
///
#[account(zero_copy)]
pub struct Pool {
    pub authority: Pubkey,
    /// Mint of the token deposited as collateral (tracked raw, no LP issued).
    pub collateral_mint: Pubkey,
    /// Mint of the token lenders deposit and borrowers receive.
    pub lend_mint: Pubkey,
    /// LP token mint issued to lend-side depositors.
    pub lp_mint: Pubkey,
    pub market: Market,
    /// IRM program ID. All borrow-rate queries are made via CPI to this program.
    pub rate_program: Pubkey,
    /// IRM state account (PDA) passed to the rate program CPI.
    pub irm_state: Pubkey,
    /// Feed program ID used for price oracle queries.
    pub feed_program: Pubkey,
    /// Feed state account (PDA) passed to the feed program.
    pub feed_state: Pubkey,
    pub lp_mint_bump: u8,
    _pad: [u8; 7], // explicit padding — no implicit/uninitialised bytes
    /// Queue of pending lend-token withdrawals (LP burned at `leave` time).
    pub withdrawal_queue: WithdrawalQueue,
}

impl Pool {
    pub fn calculate_utilization(&self) -> u64 {
        jbl_math::utilization_bps(
            self.market.total_supply_assets,
            self.market.total_borrow_assets,
            self.market.assets_in_queue,
        )
    }
}

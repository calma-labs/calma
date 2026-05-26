use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct IrmState {
    /// The pool this IRM state is bound to.
    pub pool: Pubkey,
    /// Borrow rate at 0% utilization (basis points).
    pub min_rate_bps: u32,
    /// Borrow rate at 100% utilization (basis points).
    pub max_rate_bps: u32,
    pub bump: u8,
}

#[account(zero_copy)]
pub struct IrmConfig {
    pub pool: Pubkey,
    pub a: u64,
    pub b: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

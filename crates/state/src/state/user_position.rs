use anchor_lang::prelude::*;
use math::Position;

/// Tracks a user's collateral deposit and any open borrow position.
/// Created on first collateral deposit; borrow fields populated when the user borrows.
#[account(zero_copy)]
pub struct UserPosition {
    /// The user who owns this position
    pub authority: Pubkey,
    /// The pool this position belongs to
    pub pool: Pubkey,
    /// Raw amount of collateral tokens deposited (no shares — tracked 1:1).
    pub collateral_deposited: u64,
    /// Debt shares held by this user (0 if no active borrow).
    /// Current debt = debt_shares * pool.market.total_borrow_assets
    ///                            / pool.market.total_borrow_shares
    pub debt_shares: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

impl Position for UserPosition {
    fn collateral_deposited(&self) -> u64 {
        self.collateral_deposited
    }
    fn debt_shares(&self) -> u64 {
        self.debt_shares
    }
    fn collateral_deposited_mut(&mut self) -> &mut u64 {
        &mut self.collateral_deposited
    }
    fn debt_shares_mut(&mut self) -> &mut u64 {
        &mut self.debt_shares
    }
}

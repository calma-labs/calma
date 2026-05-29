use anchor_lang::prelude::*;
use jbl_math::Position;

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

impl Position for UserPosition {
    fn collateral_deposited(&self) -> u64 {
        self.collateral_deposited
    }
    fn debt_shares(&self) -> u64 {
        self.debt_shares
    }
    fn set_collateral_deposited(&mut self, value: u64) {
        self.collateral_deposited = value;
    }
    fn set_debt_shares(&mut self, value: u64) {
        self.debt_shares = value;
    }
}

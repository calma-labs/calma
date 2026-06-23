use anchor_lang::prelude::*;

pub const MAX_WHITELIST: usize = 64;

#[account]
pub struct GuardState {
    pub authority: Pubkey,
    pub whitelist: Vec<Pubkey>,
    pub bump: u8,
}

impl GuardState {
    pub const SPACE: usize = 8 + 32 + 4 + MAX_WHITELIST * 32 + 1;
}

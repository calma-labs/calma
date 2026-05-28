use anchor_lang::prelude::*;

#[account]
pub struct Feed {
    pub authority: Pubkey,
    pub value: u64,
    pub bump: u8,
}

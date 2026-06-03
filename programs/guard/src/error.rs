use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Pubkey is not in the whitelist")]
    NotWhitelisted,
    #[msg("Pubkey is already in the whitelist")]
    AlreadyWhitelisted,
    #[msg("Whitelist is full")]
    WhitelistFull,
}

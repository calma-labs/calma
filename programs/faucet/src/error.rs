use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid amount provided")]
    InvalidAmount,
    #[msg("Signer is not authorized for this account")]
    Unauthorized,
    #[msg("Provided mint does not match the expected mint")]
    InvalidMint,
    #[msg("Mint symbol must be between 1 and 8 bytes")]
    InvalidSymbol,
}

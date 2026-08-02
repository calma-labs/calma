use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Signer is not this provider's authority")]
    Unauthorized,
    #[msg("Price must be greater than zero")]
    ZeroPrice,
    #[msg("price_ttl_ms must be greater than zero — zero rejects every consumer")]
    InvalidPriceTtl,
    #[msg("Borrow rate exceeds the maximum this provider will quote")]
    RateTooHigh,
}

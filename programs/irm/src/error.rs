use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Invalid point list (need 2..=4 points, first util_bps==0, strictly increasing util_bps)")]
    InvalidPointList,
    #[msg("Signer is not the IRM authority")]
    Unauthorized,
    #[msg("Rate point exceeds the maximum allowed borrow rate")]
    RateTooHigh,
}

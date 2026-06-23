use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Curve index out of range (must be 0..3)")]
    InvalidCurveIndex,
    #[msg("Signer is not the IRM authority")]
    Unauthorized,
}

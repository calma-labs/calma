use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Feed source does not match the requested operation")]
    WrongSource,
    #[msg("Pyth price is non-positive")]
    NonPositivePrice,
    #[msg("Price scaling overflowed")]
    PriceOverflow,
    #[msg("Price must be greater than zero")]
    ZeroPrice,
    #[msg("Feed ID is invalid for the configured source")]
    InvalidFeedId,
    #[msg("max_pyth_age_secs must be greater than zero for Pyth feeds")]
    InvalidMaxPythAge,
}

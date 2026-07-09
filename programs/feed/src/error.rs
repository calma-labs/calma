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
    #[msg("Pyth confidence exceeds max_conf_bps")]
    ConfidenceTooWide,
    #[msg("Normalized price is outside the configured [min_price, max_price] bounds")]
    PriceOutOfBounds,
    #[msg("Spot price diverges from Pyth EMA by more than ema_divergence_bps")]
    EmaDivergenceTooLarge,
    #[msg("Price change exceeds the time-scaled deviation budget")]
    PriceDeviationTooLarge,
    #[msg("FeedRules values are inconsistent (e.g. min_price > max_price)")]
    InvalidRules,
    #[msg("Push price update account did not match the sponsored feed pubkey")]
    InvalidPushAccount,
    #[msg("Push oracle price is older than max_pyth_age_secs")]
    StalePushPrice,
}

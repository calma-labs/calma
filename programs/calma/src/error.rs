// Re-exported — original definition is in crates/state/src/error.rs
pub use state::error::ErrorCode;

use anchor_lang::prelude::*;

#[error_code]
pub enum CalmaError {
    #[msg("Oracle price is older than the feed's own price_ttl_ms")]
    StaleOracle,
    #[msg("Oracle reports a zero lend price, so no ratio can be derived")]
    ZeroPrice,
}

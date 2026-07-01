// Re-exported — original definition is in crates/state/src/error.rs
pub use state::error::ErrorCode;

use anchor_lang::prelude::*;

#[error_code]
pub enum JblError {
    #[msg("Oracle feed snapshot is older than the maximum allowed age")]
    StaleOracle,
}

use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

/// Seconds per year (365.25 days)
pub const SECONDS_PER_YEAR: u64 = 31_557_600;

/// Maximum protocol fee rate: 25% of accrued interest (2_500 bps).
pub const MAX_FEE_BPS: u64 = 2_500;

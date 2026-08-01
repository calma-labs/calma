use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

/// Seconds per year (365.25 days)
pub const SECONDS_PER_YEAR: u64 = 31_557_600;

// Protocol limits (`MAX_LTV_PERCENT`) and their predicates live in
// `crates/math` alongside the rules that depend on them, so the program and the
// client bindings validate against one definition.

use anchor_lang::prelude::*;

/// PDA seed prefix for the faucet's single mint authority: `["mint_authority"]`.
///
/// `#[constant]` emits it into the generated IDL, which is how the TypeScript
/// tests read it rather than retyping the literal — the same mechanism
/// `calma::POOL_SPACE` uses.
#[constant]
pub const MINT_AUTHORITY_SEED: &str = "mint_authority";

/// PDA seed prefix for a faucet-issued mint: `["mint", symbol]`.
#[constant]
pub const MINT_SEED: &str = "mint";

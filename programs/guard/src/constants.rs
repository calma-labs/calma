use anchor_lang::prelude::*;

/// PDA seed prefix for a whitelist: `["guard", authority]`.
///
/// `#[constant]` emits this into the generated IDL, which is how the frontend
/// and the TypeScript tests read it instead of retyping the literal — the same
/// mechanism `calma::POOL_SPACE` uses. Guard has no `*-state` library crate for
/// the wasm bindings to reach, so the IDL is the seam.
#[constant]
pub const GUARD_SEED: &str = "guard";

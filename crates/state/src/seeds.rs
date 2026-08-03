//! PDA seed prefixes for the accounts `calma` owns.
//!
//! These are part of the program's address derivation, so every client that
//! computes one of these addresses has to agree with the program byte-for-byte.
//! Before this module the strings were written out at each `seeds = [...]` site
//! and again in TypeScript — three times in `app/` and once more in
//! `packages/test/` — so renaming one on-chain left the clients deriving an
//! address that simply does not exist, with nothing failing until a transaction
//! did.
//!
//! Exposed to the browser through the `*_seed()` functions in `bindings`.

/// The protocol's single signing authority PDA: `["state"]`.
pub const STATE: &[u8] = b"state";

/// A market's collateral vault: `["collateral_vault", pool]`.
pub const COLLATERAL_VAULT: &[u8] = b"collateral_vault";

/// A market's lend vault: `["lend_vault", pool]`.
pub const LEND_VAULT: &[u8] = b"lend_vault";

/// A market's LP mint: `["lp_mint", pool]`.
pub const LP_MINT: &[u8] = b"lp_mint";

/// A borrower's position in a market: `["user_position", pool, authority]`.
pub const USER_POSITION: &[u8] = b"user_position";

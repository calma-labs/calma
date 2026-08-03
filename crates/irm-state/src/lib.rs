use anchor_lang::prelude::*;

// Declare the irm program ID so #[account(zero_copy)] can resolve `ID` in the crate root.
declare_id!("irmdacogiedKeCEBh72FJx4aoixyaByqGikTkxGifUk");

/// Re-exported from [`interface`], which owns the rate-provider contract.
///
/// Defined there rather than here because `calma` has to derive the same address
/// and cannot depend on this crate — it carries the reference IRM's program ID,
/// and the lending program links no provider. Kept as a re-export so
/// `irm_state::IRM_CONFIG_SEED` keeps resolving for this program, the bindings
/// and the test fixtures.
pub use interface::IRM_CONFIG_SEED;

pub mod fees;
pub mod state;

pub use fees::*;
pub use state::*;

use anchor_lang::prelude::*;

// The state types are bound to this program — declare the same ID so the
// #[account(zero_copy)] macro can resolve `ID` in the crate root.
declare_id!("c1md3yLhwBxREDRc2HcX4ivTigoyJ3No3DehkzjaPT8");

pub mod error;
pub mod seeds;
pub mod state;
pub mod withdrawal_queue;

pub use error::ErrorCode;
pub use state::*;
pub use withdrawal_queue::*;

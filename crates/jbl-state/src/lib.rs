use anchor_lang::prelude::*;

// The state types are bound to this program — declare the same ID so the
// #[account(zero_copy)] macro can resolve `ID` in the crate root.
declare_id!("kG6m2LuBypnLDKcxu1TTXC9BrWYy4o3V71kVuHv73mg");

pub mod error;
pub mod fees;
pub mod state;
pub mod withdrawal_queue;

pub use error::ErrorCode;
pub use fees::*;
pub use state::*;
pub use withdrawal_queue::*;

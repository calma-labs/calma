use anchor_lang::prelude::*;

// The state types are bound to this program — declare the same ID so the
// #[account(zero_copy)] macro can resolve `ID` in the crate root.
declare_id!("D33b3Lb42BGyUQZyftS52idrwsjMy8jYqjb6V4ow1RxD");

pub mod error;
pub mod state;
pub mod withdrawal_queue;

pub use error::ErrorCode;
pub use state::*;
pub use withdrawal_queue::*;

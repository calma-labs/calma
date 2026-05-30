use anchor_lang::prelude::*;

// Declare the irm program ID so #[account(zero_copy)] can resolve `ID` in the crate root.
declare_id!("3o6qREQVSmDwiny77YaTWdDhVKC9YRkQEX7eydid9pFt");

pub mod fees;
pub mod state;

pub use fees::*;
pub use state::*;

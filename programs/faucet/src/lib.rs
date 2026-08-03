//! Test-only faucet: creates test mints, hands out their tokens on request, and
//! offers a 1:1 burn/mint "swap" used by local validators and integration tests
//! to move a user between the two mints of a test pool.
//!
//! This lives in its own program on purpose. It was previously an instruction
//! on `calma` carrying the comment "It must never be deployed to mainnet" —
//! which is not something a comment can enforce when the instruction ships
//! inside the lending program's binary. Splitting it means the production
//! program simply does not contain it, and deploying the faucet is a separate,
//! visible decision.
//!
//! Every instruction works against mints owned by the program's
//! `[b"mint_authority"]` PDA: the faucet can only ever issue tokens of mints it
//! was handed authority over, and no private key has to be shipped to clients
//! for them to mint or swap.

pub mod constants;
pub mod error;
pub mod instructions;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::ErrorCode;
pub use instructions::*;

declare_id!("HALzjfshwyYKYjLNL6ectL9oBoM3tLcUCAabZNYrxwWy");

#[program]
pub mod faucet {
    use super::*;

    pub fn create(ctx: Context<Create>, symbol: String, decimals: u8) -> Result<()> {
        create_handler(ctx, symbol, decimals)
    }

    pub fn mint(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
        mint_handler(ctx, amount)
    }

    pub fn mock_swap(ctx: Context<MockSwap>, amount: u64) -> Result<()> {
        mock_swap_handler(ctx, amount)
    }
}

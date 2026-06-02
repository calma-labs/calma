pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::ErrorCode;
pub use instructions::*;
pub use state::*;

declare_id!("FyJvcrTMrMbzSmqTkwY7h8AkVR4BYq2pQrfgXmN1jh5M");

#[program]
pub mod guard {
    use super::*;

    pub fn create(ctx: Context<Create>) -> Result<()> {
        initialize::create_handler(ctx)
    }

    pub fn add(ctx: Context<Add>, pubkey: Pubkey) -> Result<()> {
        add::add_handler(ctx, pubkey)
    }

    pub fn remove(ctx: Context<Remove>, pubkey: Pubkey) -> Result<()> {
        remove::remove_handler(ctx, pubkey)
    }

    pub fn check(ctx: Context<Check>, pubkey: Pubkey) -> Result<()> {
        check::check_handler(ctx, pubkey)
    }
}

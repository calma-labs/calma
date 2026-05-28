pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("BP5cC1PQDrjX74QjXU8PMbDBje3oiaVJAh3hJS9AtyhU");

#[program]
pub mod feed {
    use super::*;

    pub fn create(ctx: Context<Create>) -> Result<()> {
        create::create_handler(ctx)
    }

    pub fn set_value(ctx: Context<SetValue>, value: u64) -> Result<()> {
        set_value::set_value_handler(ctx, value)
    }
}

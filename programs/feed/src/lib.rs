pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("EApGqVemyo71RVeYSwJpGh8w5YMG6zDk41pLonnHuuSK");

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

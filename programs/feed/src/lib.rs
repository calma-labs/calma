pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4");

#[program]
pub mod feed {
    use super::*;

    pub fn create(ctx: Context<Create>) -> Result<()> {
        create::create_handler(ctx)
    }

    pub fn set_value(ctx: Context<SetValue>, value: u64) -> Result<()> {
        set_value::set_value_handler(ctx, value)
    }

    pub fn get_value(ctx: Context<GetValue>) -> Result<u64> {
        get_value::get_value_handler(ctx)
    }
}

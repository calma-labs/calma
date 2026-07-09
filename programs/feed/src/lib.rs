pub mod constants;
pub mod error;
pub mod instructions;
pub mod pyth;
pub mod rules;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4");

#[program]
pub mod feed {
    use super::*;

    pub fn create(
        ctx: Context<Create>,
        source: PriceSource,
        collateral_feed_id: [u8; 32],
        lend_feed_id: [u8; 32],
        max_pyth_age_secs: u32,
        rules: FeedRules,
    ) -> Result<()> {
        create::create_handler(
            ctx,
            source,
            collateral_feed_id,
            lend_feed_id,
            max_pyth_age_secs,
            rules,
        )
    }

    pub fn set_value(
        ctx: Context<SetValue>,
        collateral_price: u64,
        lend_price: u64,
    ) -> Result<()> {
        set_value::set_value_handler(ctx, collateral_price, lend_price)
    }

    pub fn set_from_pyth(ctx: Context<SetFromPyth>) -> Result<()> {
        set_from_pyth::set_from_pyth_handler(ctx)
    }

    pub fn set_from_pyth_push(ctx: Context<SetFromPythPush>) -> Result<()> {
        set_from_pyth_push::set_from_pyth_push_handler(ctx)
    }

    pub fn get_value(ctx: Context<GetValue>) -> Result<u64> {
        get_value::get_value_handler(ctx)
    }

    pub fn get_state(ctx: Context<GetValue>) -> Result<FeedSnapshot> {
        get_value::get_state_handler(ctx)
    }
}

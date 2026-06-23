use crate::state::Feed;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct GetValue<'info> {
    pub feed: Account<'info, Feed>,
}

pub fn get_value_handler(ctx: Context<GetValue>) -> Result<u64> {
    Ok(ctx.accounts.feed.value)
}

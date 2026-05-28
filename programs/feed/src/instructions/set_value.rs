use crate::state::Feed;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetValue<'info> {
    #[account(
        mut,
        seeds = [b"feed", authority.key().as_ref()],
        bump = feed.bump,
        has_one = authority,
    )]
    pub feed: Account<'info, Feed>,

    pub authority: Signer<'info>,
}

pub fn set_value_handler(ctx: Context<SetValue>, value: u64) -> Result<()> {
    ctx.accounts.feed.value = value;
    Ok(())
}

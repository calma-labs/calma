use crate::state::Feed;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Create<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<Feed>(),
        seeds = [b"feed", authority.key().as_ref()],
        bump,
    )]
    pub feed: Account<'info, Feed>,

    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_handler(ctx: Context<Create>) -> Result<()> {
    let feed = &mut ctx.accounts.feed;
    feed.authority = ctx.accounts.authority.key();
    feed.value = 0;
    feed.bump = ctx.bumps.feed;
    Ok(())
}

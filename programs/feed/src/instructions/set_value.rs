use crate::error::ErrorCode;
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetValue<'info> {
    #[account(
        mut,
        seeds = [
            b"feed",
            feed.collateral_mint.as_ref(),
            feed.lend_mint.as_ref(),
            &[feed.id],
        ],
        bump = feed.config.bump,
        constraint = feed.config.authority == authority.key(),
    )]
    pub feed: Account<'info, Feed>,

    pub authority: Signer<'info>,
}

pub fn set_value_handler(
    ctx: Context<SetValue>,
    collateral_price: u64,
    lend_price: u64,
) -> Result<()> {
    let feed = &mut ctx.accounts.feed;
    require!(
        feed.config.source == PriceSource::Manual,
        ErrorCode::WrongSource
    );
    require!(
        collateral_price > 0 && lend_price > 0,
        ErrorCode::ZeroPrice
    );

    feed.state.collateral_price = collateral_price;
    feed.state.lend_price = lend_price;
    feed.state.last_updated_ts = Clock::get()?.unix_timestamp;
    Ok(())
}

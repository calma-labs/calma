use crate::error::ErrorCode;
use crate::pyth::normalize;
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

#[derive(Accounts)]
pub struct SetFromPyth<'info> {
    #[account(
        mut,
        seeds = [b"feed", feed.config.authority.as_ref()],
        bump = feed.config.bump,
    )]
    pub feed: Account<'info, Feed>,

    pub collateral_price_update: Account<'info, PriceUpdateV2>,
    pub lend_price_update: Account<'info, PriceUpdateV2>,
}

pub fn set_from_pyth_handler(ctx: Context<SetFromPyth>) -> Result<()> {
    let feed = &mut ctx.accounts.feed;
    require!(
        feed.config.source == PriceSource::Pyth,
        ErrorCode::WrongSource
    );

    let clock = Clock::get()?;
    let max_age = feed.config.max_pyth_age_secs as u64;

    let coll = ctx.accounts.collateral_price_update.get_price_no_older_than(
        &clock,
        max_age,
        &feed.config.collateral_feed_id,
    )?;
    let lend = ctx.accounts.lend_price_update.get_price_no_older_than(
        &clock,
        max_age,
        &feed.config.lend_feed_id,
    )?;

    let coll_norm = normalize(coll.price, coll.exponent)?;
    let lend_norm = normalize(lend.price, lend.exponent)?;

    feed.state.collateral_price = coll_norm;
    feed.state.lend_price = lend_norm;
    feed.state.last_updated_ts = coll.publish_time.min(lend.publish_time);
    Ok(())
}

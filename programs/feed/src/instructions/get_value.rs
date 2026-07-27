use crate::constants::PRICE_SCALE;
use crate::error::ErrorCode;
use crate::state::{Feed, FeedSnapshot};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct GetValue<'info> {
    pub feed: Account<'info, Feed>,
}

fn compute_snapshot(feed: &Feed) -> Result<FeedSnapshot> {
    require!(feed.state.lend_price > 0, ErrorCode::ZeroPrice);

    let coll_price = feed.state.collateral_price as u128;
    let lend_price = feed.state.lend_price as u128;

    let coll_dec_pow = 10u128
        .checked_pow(feed.data.collateral_decimals as u32)
        .ok_or(ErrorCode::PriceOverflow)?;
    let lend_dec_pow = 10u128
        .checked_pow(feed.data.lend_decimals as u32)
        .ok_or(ErrorCode::PriceOverflow)?;

    let numerator = coll_price
        .checked_mul(lend_dec_pow)
        .ok_or(ErrorCode::PriceOverflow)?
        .checked_mul(PRICE_SCALE)
        .ok_or(ErrorCode::PriceOverflow)?;
    let denominator = lend_price
        .checked_mul(coll_dec_pow)
        .ok_or(ErrorCode::PriceOverflow)?;
    require!(denominator > 0, ErrorCode::ZeroPrice);

    let ratio_u128 = numerator
        .checked_div(denominator)
        .ok_or(ErrorCode::PriceOverflow)?;
    let ratio = u64::try_from(ratio_u128).map_err(|_| ErrorCode::PriceOverflow)?;

    Ok(FeedSnapshot {
        ratio,
        last_updated_ts: feed.state.last_updated_ts,
    })
}

pub fn get_value_handler(ctx: Context<GetValue>) -> Result<u64> {
    Ok(compute_snapshot(&ctx.accounts.feed)?.ratio)
}

pub fn get_state_handler(ctx: Context<GetValue>) -> Result<FeedSnapshot> {
    compute_snapshot(&ctx.accounts.feed)
}

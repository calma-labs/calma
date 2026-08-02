use crate::error::ErrorCode;
use crate::pyth::normalize;
use crate::rules::{check_bounds, check_conf, check_deviation, check_ema_divergence, check_max_age};
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

#[derive(Accounts)]
pub struct SetFromPyth<'info> {
    #[account(
        mut,
        seeds = [
            b"feed",
            feed.header.collateral_mint.as_ref(),
            feed.header.lend_mint.as_ref(),
            &[feed.id],
        ],
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

    // `get_price_unchecked` verifies the feed id but leaves freshness to us. The
    // SDK's age check takes whole seconds, so routing through it required
    // `max_age_ms / 1_000` — truncating any sub-second budget to 0 (which
    // rejects every price and bricks the feed) and any 1_500 ms budget to
    // 1_000. `check_max_age` applies the configured value in milliseconds, and
    // is the same gate the push path uses.
    let coll = ctx
        .accounts
        .collateral_price_update
        .get_price_unchecked(&feed.config.collateral_feed_id)?;
    let lend = ctx
        .accounts
        .lend_price_update
        .get_price_unchecked(&feed.config.lend_feed_id)?;
    check_max_age(coll.publish_time, clock.unix_timestamp, feed.rules.max_age_ms)?;
    check_max_age(lend.publish_time, clock.unix_timestamp, feed.rules.max_age_ms)?;

    let coll_norm = normalize(coll.price, coll.exponent)?;
    let lend_norm = normalize(lend.price, lend.exponent)?;

    // Rule gates. `conf` and `ema_price` share the price exponent, so they
    // are compared against *raw* Pyth values (no double-scaling); bounds and
    // deviation act on the PRICE_SCALE-normalized values that will actually
    // be written to state.
    let rules = &feed.rules;
    // `Price` returned by the SDK strips the EMA fields, so pull them off the
    // underlying account. Matching feed IDs are already enforced by
    // `get_price_no_older_than` above.
    let coll_ema = ctx
        .accounts
        .collateral_price_update
        .price_message
        .ema_price;
    let lend_ema = ctx.accounts.lend_price_update.price_message.ema_price;
    check_conf(coll.price, coll.conf, rules.max_conf_bps)?;
    check_conf(lend.price, lend.conf, rules.max_conf_bps)?;
    check_bounds(coll_norm, rules.min_price, rules.max_price)?;
    check_bounds(lend_norm, rules.min_price, rules.max_price)?;
    check_ema_divergence(coll.price, coll_ema, rules.ema_divergence_bps)?;
    check_ema_divergence(lend.price, lend_ema, rules.ema_divergence_bps)?;
    check_deviation(
        coll_norm,
        feed.header.collateral_price,
        feed.header.last_updated_ts,
        clock.unix_timestamp,
        rules.max_deviation_bps_per_hour,
    )?;
    check_deviation(
        lend_norm,
        feed.header.lend_price,
        feed.header.last_updated_ts,
        clock.unix_timestamp,
        rules.max_deviation_bps_per_hour,
    )?;

    feed.header.collateral_price = coll_norm;
    feed.header.lend_price = lend_norm;
    feed.header.last_updated_ts = coll.publish_time.min(lend.publish_time);
    Ok(())
}

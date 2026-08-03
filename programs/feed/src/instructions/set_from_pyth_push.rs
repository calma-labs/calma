use crate::error::ErrorCode;
use crate::pyth::normalize;
use crate::rules::{
    check_bounds, check_conf, check_deviation, check_ema_divergence, check_max_age,
    check_monotonic,
};
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{PriceUpdateV2, VerificationLevel};

/// Consume a **sponsored** Pyth push feed. The `PriceUpdateV2` account layout is
/// identical to the pull path (owner check via `Account<'info, PriceUpdateV2>`),
/// but here we pin to a specific sponsored account pubkey stored in the feed
/// config rather than matching an internal Pyth feed_id hash.
#[derive(Accounts)]
pub struct SetFromPythPush<'info> {
    #[account(
        mut,
        seeds = [
            ::feed_state::FEED_SEED,
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

pub fn set_from_pyth_push_handler(ctx: Context<SetFromPythPush>) -> Result<()> {
    let feed = &mut ctx.accounts.feed;
    require!(
        feed.config.source == PriceSource::PythPush,
        ErrorCode::WrongSource
    );

    // For sponsored push feeds we pin by account pubkey (stored in the
    // `*_feed_id` slots at feed creation) rather than by Pyth feed_id hash.
    require!(
        ctx.accounts.collateral_price_update.key().to_bytes() == feed.config.collateral_feed_id,
        ErrorCode::InvalidPushAccount
    );
    require!(
        ctx.accounts.lend_price_update.key().to_bytes() == feed.config.lend_feed_id,
        ErrorCode::InvalidPushAccount
    );

    // Guardian quorum — same requirement as the pull path, and needed more here.
    // This path reads `price_message` straight off the account rather than going
    // through a getter, so it inherits no checks at all: not the verification
    // level, and not the embedded Pyth feed id.
    //
    // The feed id genuinely cannot be checked on this path — the `*_feed_id`
    // config slots are reused to hold the sponsored *account pubkey*, which is
    // what the pin above compares. That pin is only as strong as the account
    // being a real Pyth-maintained sponsored feed, so the quorum check is the
    // one thing that can still be asserted about the data itself.
    require!(
        ctx.accounts
            .collateral_price_update
            .verification_level
            .gte(VerificationLevel::Full),
        ErrorCode::InsufficientVerification
    );
    require!(
        ctx.accounts
            .lend_price_update
            .verification_level
            .gte(VerificationLevel::Full),
        ErrorCode::InsufficientVerification
    );

    let coll = ctx.accounts.collateral_price_update.price_message;
    let lend = ctx.accounts.lend_price_update.price_message;

    let clock = Clock::get()?;
    check_max_age(coll.publish_time, clock.unix_timestamp, feed.rules.max_age_ms)?;
    check_max_age(lend.publish_time, clock.unix_timestamp, feed.rules.max_age_ms)?;

    // Same reasoning as the pull path: this instruction is permissionless, so
    // the written price must never move backwards in time.
    let new_ts = coll.publish_time.min(lend.publish_time);
    check_monotonic(new_ts, feed.header.last_updated_ts)?;

    let coll_norm = normalize(coll.price, coll.exponent)?;
    let lend_norm = normalize(lend.price, lend.exponent)?;

    let rules = &feed.rules;
    check_conf(coll.price, coll.conf, rules.max_conf_bps)?;
    check_conf(lend.price, lend.conf, rules.max_conf_bps)?;
    check_bounds(coll_norm, rules.min_price, rules.max_price)?;
    check_bounds(lend_norm, rules.min_price, rules.max_price)?;
    check_ema_divergence(coll.price, coll.ema_price, rules.ema_divergence_bps)?;
    check_ema_divergence(lend.price, lend.ema_price, rules.ema_divergence_bps)?;
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
    feed.header.last_updated_ts = new_ts;
    Ok(())
}

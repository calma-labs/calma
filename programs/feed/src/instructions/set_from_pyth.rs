use crate::error::ErrorCode;
use crate::pyth::normalize;
use crate::rules::{
    check_bounds, check_conf, check_deviation, check_ema_divergence, check_max_age,
    check_monotonic,
};
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{PriceUpdateV2, VerificationLevel};

#[derive(Accounts)]
pub struct SetFromPyth<'info> {
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

pub fn set_from_pyth_handler(ctx: Context<SetFromPyth>) -> Result<()> {
    let feed = &mut ctx.accounts.feed;
    require!(
        feed.config.source == PriceSource::Pyth,
        ErrorCode::WrongSource
    );

    let clock = Clock::get()?;

    // ── Guardian quorum ───────────────────────────────────────────────────────
    //
    // `get_price_unchecked` skips **two** checks, not one. Its name reads as
    // "unchecked age", and the note below covers why taking freshness into our
    // own hands is deliberate — but it also drops the verification-level check,
    // which in the SDK lives only inside
    // `get_price_no_older_than_with_custom_verification_level`. Bypassing the
    // age check there took the quorum check with it.
    //
    // Posting a `PriceUpdateV2` is permissionless, and one posted with
    // `VerificationLevel::Partial` carries a smaller guardian subset than a full
    // quorum. The SDK's own warning: "Lowering the verification level from
    // `Full` to `Partial` increases the risk of using a malicious price update."
    // A market prices its entire collateral book off this account, so `Full` is
    // the only acceptable level. Asserted here, explicitly, rather than being
    // inherited from whichever getter happens to be called.
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

    // Relaying is permissionless, so the caller — not the publisher — picks
    // which signed update inside the ingestion window gets written. Refusing an
    // update older than the one already stored takes the backwards half of that
    // choice away; see `check_monotonic`.
    let new_ts = coll.publish_time.min(lend.publish_time);
    check_monotonic(new_ts, feed.header.last_updated_ts)?;

    let coll_norm = normalize(coll.price, coll.exponent)?;
    let lend_norm = normalize(lend.price, lend.exponent)?;

    // Rule gates. `conf` and `ema_price` share the price exponent, so they
    // are compared against *raw* Pyth values (no double-scaling); bounds and
    // deviation act on the PRICE_SCALE-normalized values that will actually
    // be written to state.
    let rules = &feed.rules;
    // `Price` returned by the SDK strips the EMA fields, so pull them off the
    // underlying account. Reading them raw is safe because the feed ids were
    // matched by `get_price_unchecked` above — that is the one check it does
    // perform. It used to say `get_price_no_older_than` here, which is not
    // called anywhere in this file; that mistaken mental model is exactly what
    // left the verification level unchecked until it was audited.
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
    feed.header.last_updated_ts = new_ts;
    Ok(())
}

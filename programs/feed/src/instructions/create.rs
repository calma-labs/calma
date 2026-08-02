use crate::error::ErrorCode;
use crate::rules::validate_rules;
use crate::state::{Feed, FeedRules, PriceSource};
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

#[derive(Accounts)]
#[instruction(id: u8)]
pub struct Create<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<Feed>(),
        seeds = [
            b"feed",
            collateral_mint.key().as_ref(),
            lend_mint.key().as_ref(),
            &[id],
        ],
        bump,
    )]
    pub feed: Account<'info, Feed>,

    pub authority: Signer<'info>,

    pub collateral_mint: Account<'info, Mint>,
    pub lend_mint: Account<'info, Mint>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_handler(
    ctx: Context<Create>,
    id: u8,
    source: PriceSource,
    collateral_feed_id: [u8; 32],
    lend_feed_id: [u8; 32],
    price_ttl_ms: u32,
    rules: FeedRules,
) -> Result<()> {
    // The consumption budget must be a real one. `0` is the fail-closed
    // sentinel in `interface::price_stale_at`, so a feed created with it would
    // reject every borrow against it — refuse here rather than ship a feed that
    // silently cannot be used.
    require!(price_ttl_ms > 0, ErrorCode::InvalidPriceTtl);

    let zero = [0u8; 32];
    match source {
        PriceSource::Manual => {
            require!(
                collateral_feed_id == zero && lend_feed_id == zero,
                ErrorCode::InvalidFeedId
            );
        }
        PriceSource::Pyth | PriceSource::PythPush => {
            require!(
                collateral_feed_id != zero && lend_feed_id != zero,
                ErrorCode::InvalidFeedId
            );
            require!(rules.max_age_ms > 0, ErrorCode::InvalidMaxPythAge);
            validate_rules(&rules)?;
        }
    }

    let feed = &mut ctx.accounts.feed;
    // Prices start at 0 with a `last_updated_ts` of 0, so the feed reads as
    // stale to consumers until its first `set_value` / `set_from_pyth`.
    feed.header = crate::state::PriceFeedHeader {
        collateral_mint: ctx.accounts.collateral_mint.key(),
        lend_mint: ctx.accounts.lend_mint.key(),
        collateral_price: 0,
        lend_price: 0,
        collateral_decimals: ctx.accounts.collateral_mint.decimals,
        lend_decimals: ctx.accounts.lend_mint.decimals,
        last_updated_ts: 0,
        price_ttl_ms,
    };
    feed.id = id;
    feed.config = crate::state::FeedConfig {
        authority: ctx.accounts.authority.key(),
        source,
        bump: ctx.bumps.feed,
        _pad: [0; 6],
        collateral_feed_id,
        lend_feed_id,
    };
    // Manual feeds accept any rules value but the enforcement site
    // (`set_from_pyth`) is unreachable, so the values are inert.
    feed.rules = rules;
    Ok(())
}

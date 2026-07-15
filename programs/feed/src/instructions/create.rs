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
    rules: FeedRules,
) -> Result<()> {
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
    feed.collateral_mint = ctx.accounts.collateral_mint.key();
    feed.lend_mint = ctx.accounts.lend_mint.key();
    feed.id = id;
    feed.config = crate::state::FeedConfig {
        authority: ctx.accounts.authority.key(),
        source,
        bump: ctx.bumps.feed,
        _pad: [0; 6],
        collateral_feed_id,
        lend_feed_id,
    };
    feed.data = crate::state::FeedData {
        collateral_decimals: ctx.accounts.collateral_mint.decimals,
        lend_decimals: ctx.accounts.lend_mint.decimals,
    };
    feed.state = crate::state::FeedState {
        collateral_price: 0,
        lend_price: 0,
        last_updated_ts: 0,
    };
    // Manual feeds accept any rules value but the enforcement site
    // (`set_from_pyth`) is unreachable, so the values are inert.
    feed.rules = rules;
    Ok(())
}

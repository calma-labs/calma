use crate::error::ErrorCode;
use crate::state::{Feed, PriceSource};
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

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

    pub collateral_mint: Account<'info, Mint>,
    pub lend_mint: Account<'info, Mint>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_handler(
    ctx: Context<Create>,
    source: PriceSource,
    collateral_feed_id: [u8; 32],
    lend_feed_id: [u8; 32],
    max_pyth_age_secs: u32,
) -> Result<()> {
    let zero = [0u8; 32];
    let stored_max_pyth_age = match source {
        PriceSource::Manual => {
            require!(
                collateral_feed_id == zero && lend_feed_id == zero,
                ErrorCode::InvalidFeedId
            );
            // Field is meaningless for Manual feeds — store 0 explicitly.
            0
        }
        PriceSource::Pyth => {
            require!(
                collateral_feed_id != zero && lend_feed_id != zero,
                ErrorCode::InvalidFeedId
            );
            require!(max_pyth_age_secs > 0, ErrorCode::InvalidMaxPythAge);
            max_pyth_age_secs
        }
    };

    let feed = &mut ctx.accounts.feed;
    feed.config = crate::state::FeedConfig {
        authority: ctx.accounts.authority.key(),
        source,
        bump: ctx.bumps.feed,
        _pad: [0; 2],
        max_pyth_age_secs: stored_max_pyth_age,
        collateral_feed_id,
        lend_feed_id,
    };
    feed.data = crate::state::FeedData {
        collateral_mint: ctx.accounts.collateral_mint.key(),
        lend_mint: ctx.accounts.lend_mint.key(),
        collateral_decimals: ctx.accounts.collateral_mint.decimals,
        lend_decimals: ctx.accounts.lend_mint.decimals,
    };
    feed.state = crate::state::FeedState {
        collateral_price: 0,
        lend_price: 0,
        last_updated_ts: 0,
    };
    feed._reserved = [0; 30];
    Ok(())
}

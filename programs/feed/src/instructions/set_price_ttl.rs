use crate::error::ErrorCode;
use crate::state::Feed;
use anchor_lang::prelude::*;

/// Adjust how long consumers may act on this feed's price.
///
/// Separate from `create` because the right budget follows the feed's *actual*
/// update cadence, which is only known once it is running — and because a feed
/// whose publisher slows down needs a way to widen its budget without being
/// recreated, which would orphan every market pinned to its address.
#[derive(Accounts)]
pub struct SetPriceTtl<'info> {
    #[account(
        mut,
        seeds = [
            b"feed",
            feed.header.collateral_mint.as_ref(),
            feed.header.lend_mint.as_ref(),
            &[feed.id],
        ],
        bump = feed.config.bump,
        constraint = feed.config.authority == authority.key() @ ErrorCode::Unauthorized,
    )]
    pub feed: Account<'info, Feed>,

    pub authority: Signer<'info>,
}

pub fn set_price_ttl_handler(ctx: Context<SetPriceTtl>, price_ttl_ms: u32) -> Result<()> {
    // Same reasoning as `create`: `0` is the fail-closed sentinel, so setting it
    // would brick every market pinned to this feed. Refuse rather than offer a
    // one-instruction kill switch that reads like a configuration change.
    require!(price_ttl_ms > 0, ErrorCode::InvalidPriceTtl);
    ctx.accounts.feed.header.price_ttl_ms = price_ttl_ms;
    Ok(())
}

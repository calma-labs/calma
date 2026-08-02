use anchor_lang::prelude::*;

use crate::{error::ErrorCode, state::Provider, MAX_RATE_BPS};

#[derive(Accounts)]
pub struct SetValue<'info> {
    #[account(
        mut,
        seeds = [b"irm_config", provider.pool.as_ref()],
        bump = provider.bump,
        constraint = provider.authority == authority.key() @ ErrorCode::Unauthorized,
    )]
    pub provider: Account<'info, Provider>,

    pub authority: Signer<'info>,
}

/// Rewrite the price. `last_updated_ts` is stamped from the chain clock because
/// this provider *is* the publisher — a provider relaying an upstream feed would
/// carry the upstream's timestamp instead, or a stale price would be laundered
/// into a fresh one on every write.
pub fn set_price_handler(
    ctx: Context<SetValue>,
    collateral_price: u64,
    lend_price: u64,
) -> Result<()> {
    require!(
        collateral_price > 0 && lend_price > 0,
        ErrorCode::ZeroPrice
    );
    let header = &mut ctx.accounts.provider.header;
    header.collateral_price = collateral_price;
    header.lend_price = lend_price;
    header.last_updated_ts = Clock::get()?.unix_timestamp;
    Ok(())
}

/// Retune the flat rate. Takes effect on the next `borrow_rate` CPI — there is
/// no cached copy on the market side to invalidate.
pub fn set_rate_handler(ctx: Context<SetValue>, flat_rate_bps: u32) -> Result<()> {
    require!(flat_rate_bps <= MAX_RATE_BPS, ErrorCode::RateTooHigh);
    ctx.accounts.provider.flat_rate_bps = flat_rate_bps;
    Ok(())
}

/// Widen or tighten how long this provider's price stays consumable.
///
/// `0` is refused for the same reason as at `initialize`: it is the fail-closed
/// sentinel, so setting it would be a one-instruction kill switch for every
/// market pricing against this provider, dressed up as a configuration change.
pub fn set_price_ttl_handler(ctx: Context<SetValue>, price_ttl_ms: u32) -> Result<()> {
    require!(price_ttl_ms > 0, ErrorCode::InvalidPriceTtl);
    ctx.accounts.provider.header.price_ttl_ms = price_ttl_ms;
    Ok(())
}

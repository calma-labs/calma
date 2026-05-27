use anchor_lang::prelude::*;

use crate::state::IrmConfig;

#[derive(Accounts)]
pub struct BorrowRate<'info> {
    #[account(
        seeds = [b"irm_config", pool.key().as_ref()],
        bump = irm_state.load()?.bump,
        has_one = pool,
    )]
    pub irm_state: AccountLoader<'info, IrmConfig>,
    /// CHECK: verified via has_one constraint on irm_state
    pub pool: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<BorrowRate>, utilization_bps: u32) -> Result<u32> {
    let config = ctx.accounts.irm_state.load()?;
    let rate = (config.a as u128)
        .checked_mul(utilization_bps as u128)
        .and_then(|v| v.checked_div(10_000))
        .and_then(|v| v.checked_add(config.b as u128))
        .ok_or_else(|| error!(crate::error::ErrorCode::MathOverflow))?;
    let rate_u32 = u32::try_from(rate).map_err(|_| error!(crate::error::ErrorCode::MathOverflow))?;
    msg!("irm::borrow_rate utilization={} rate={}", utilization_bps, rate_u32);
    Ok(rate_u32)
}

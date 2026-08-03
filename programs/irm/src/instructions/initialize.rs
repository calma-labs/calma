use anchor_lang::prelude::*;
use irm_state::{IrmState, RatePoint, MAX_POINTS};

use crate::{error::ErrorCode, instructions::set_fee_points::RatePointArgs};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<IrmState>(),
        seeds = [::irm_state::IRM_CONFIG_SEED, pool.key().as_ref()],
        bump,
    )]
    pub irm_config: AccountLoader<'info, IrmState>,
    /// CHECK: pool is used only as a seed for PDA derivation
    pub pool: UncheckedAccount<'info>,
    /// The authority that will be allowed to update the fee curve.
    pub authority: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub(crate) fn handler(ctx: Context<Initialize>, points: Vec<RatePointArgs>) -> Result<()> {
    irm_state::validate_rate_points(
        &points.iter().map(|p| (p.util_bps, p.rate_bps)).collect::<Vec<_>>(),
    )
    .map_err(|e| match e {
        irm_state::RatePointError::InvalidPointList => ErrorCode::InvalidPointList,
        irm_state::RatePointError::RateTooHigh => ErrorCode::RateTooHigh,
    })?;
    let mut config = ctx.accounts.irm_config.load_init()?;
    config.pool = ctx.accounts.pool.key();
    config.authority = ctx.accounts.authority.key();
    config.bump = ctx.bumps.irm_config;

    let mut buf = [RatePoint::default(); MAX_POINTS];
    for (i, p) in points.iter().enumerate() {
        buf[i] = RatePoint::new(p.util_bps, p.rate_bps);
    }
    config.model.points = buf;
    config.model.len = points.len() as u8;
    Ok(())
}

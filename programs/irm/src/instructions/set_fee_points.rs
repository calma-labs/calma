use anchor_lang::prelude::*;
use irm_state::{IrmState, RatePoint, MAX_POINTS, MIN_POINTS};

use crate::error::ErrorCode;

/// One rate curve point in the wire format.  Mirrors `RatePoint` but without
/// zero-copy padding (Anchor-serialized).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct RatePointArgs {
    pub util_bps: u16,
    pub rate_bps: u32,
}

#[derive(Accounts)]
pub struct SetFeePoints<'info> {
    #[account(
        mut,
        seeds = [b"irm_config", irm_state.load()?.pool.as_ref()],
        bump = irm_state.load()?.bump,
        constraint = irm_state.load()?.authority == authority.key() @ ErrorCode::Unauthorized,
    )]
    pub irm_state: AccountLoader<'info, IrmState>,
    pub authority: Signer<'info>,
}

pub(crate) fn handler(ctx: Context<SetFeePoints>, points: Vec<RatePointArgs>) -> Result<()> {
    // All validation happens here (write time). The reader (`get_fee_bps`)
    // trusts these invariants and skips any runtime checks.
    require!(
        points.len() >= MIN_POINTS && points.len() <= MAX_POINTS,
        ErrorCode::InvalidPointList
    );
    require!(points[0].util_bps == 0, ErrorCode::InvalidPointList);
    for i in 1..points.len() {
        require!(
            points[i].util_bps > points[i - 1].util_bps,
            ErrorCode::InvalidPointList
        );
    }

    let mut state = ctx.accounts.irm_state.load_mut()?;
    let mut buf = [RatePoint::default(); MAX_POINTS];
    for (i, p) in points.iter().enumerate() {
        buf[i] = RatePoint::new(p.util_bps, p.rate_bps);
    }
    state.model.points = buf;
    state.model.len = points.len() as u8;
    Ok(())
}

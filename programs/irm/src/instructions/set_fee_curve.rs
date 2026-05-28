use anchor_lang::prelude::*;
use jbl_irm::{IrmState, LinearSegment};

use crate::error::ErrorCode;

/// Arguments for a single curve segment.  Mirrors the fields of `LinearSegment`
/// but is AnchorSerialize-compatible (not zero_copy).
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CurveArgs {
    pub a: i64,
    pub b: i64,
    pub a2: i64,
    pub kink: u64,
    pub enabled: bool,
}

#[derive(Accounts)]
pub struct SetFeeCurve<'info> {
    #[account(
        mut,
        seeds = [b"irm_config", irm_state.load()?.pool.as_ref()],
        bump = irm_state.load()?.bump,
        constraint = irm_state.load()?.authority == authority.key() @ ErrorCode::Unauthorized,
    )]
    pub irm_state: AccountLoader<'info, IrmState>,
    pub authority: Signer<'info>,
}

pub(crate) fn handler(ctx: Context<SetFeeCurve>, index: u8, curve: CurveArgs) -> Result<()> {
    require!(index < 4, ErrorCode::InvalidCurveIndex);
    let mut state = ctx.accounts.irm_state.load_mut()?;
    state.model.curves[index as usize] = LinearSegment {
        a: curve.a,
        b: curve.b,
        a2: curve.a2,
        kink: curve.kink,
        enabled: curve.enabled as u8,
        _pad: [0; 7],
    };
    Ok(())
}

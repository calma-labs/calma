use crate::state::Pool;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct EnableFeeCurve<'info> {
    #[account(
        mut,
        constraint = pool.load()?.authority == authority.key() @ crate::ErrorCode::Unauthorized,
    )]
    pub pool: AccountLoader<'info, Pool>,
    pub authority: Signer<'info>,
}

pub fn enable_fee_curve_handler(ctx: Context<EnableFeeCurve>, index: u8) -> Result<()> {
    require!(index < 4, crate::ErrorCode::InvalidCurveIndex);
    let mut pool = ctx.accounts.pool.load_mut()?;
    pool.fee_config.curves[index as usize].enabled = 1;
    Ok(())
}

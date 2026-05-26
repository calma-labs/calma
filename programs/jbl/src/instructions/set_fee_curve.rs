use crate::{fees::PolynomialCurve, instructions::create::CurveArgs, state::Pool};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetFeeCurve<'info> {
    #[account(
        mut,
        constraint = pool.load()?.authority == authority.key() @ crate::ErrorCode::Unauthorized,
    )]
    pub pool: AccountLoader<'info, Pool>,
    pub authority: Signer<'info>,
}

pub fn set_fee_curve_handler(ctx: Context<SetFeeCurve>, index: u8, curve: CurveArgs) -> Result<()> {
    require!(index < 4, crate::ErrorCode::InvalidCurveIndex);
    let mut pool = ctx.accounts.pool.load_mut()?;
    pool.fee_config.curves[index as usize] = PolynomialCurve {
        a: curve.a,
        b: curve.b,
        enabled: curve.enabled as u8,
        _pad: [0; 7],
    };
    Ok(())
}

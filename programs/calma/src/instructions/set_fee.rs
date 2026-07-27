use crate::state::Pool;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetFee<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// Must be the pool authority.
    pub authority: Signer<'info>,
}

/// Set the protocol fee rate (basis points of accrued interest). Only the pool
/// authority may call it; the rate is capped at `MAX_FEE_BPS`.
pub fn set_fee_handler(ctx: Context<SetFee>, fee_bps: u64) -> Result<()> {
    require!(
        fee_bps <= crate::constants::MAX_FEE_BPS,
        crate::error::ErrorCode::FeeTooHigh
    );
    let mut pool = ctx.accounts.pool.load_mut()?;
    require!(
        pool.authority == ctx.accounts.authority.key(),
        crate::error::ErrorCode::Unauthorized
    );
    pool.market.fee = fee_bps;
    msg!("SetFee: protocol fee set to {} bps", fee_bps);
    Ok(())
}

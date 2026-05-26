use anchor_lang::prelude::*;
use bytemuck;

use crate::state::IrmConfig;

#[derive(Accounts)]
pub struct BorrowRate<'info> {
    /// CHECK: caller must ensure this is the IrmConfig PDA for the target pool
    pub irm_state: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<BorrowRate>, utilization_bps: u32) -> Result<u32> {
    let data = ctx.accounts.irm_state.try_borrow_data()?;
    let config = bytemuck::try_from_bytes::<IrmConfig>(&data[8..])
        .map_err(|_| error!(crate::error::ErrorCode::MathOverflow))?;
    let rate = (config.a as u128)
        .checked_mul(utilization_bps as u128)
        .and_then(|v| v.checked_div(10_000))
        .and_then(|v| v.checked_add(config.b as u128))
        .ok_or_else(|| error!(crate::error::ErrorCode::MathOverflow))?;
    u32::try_from(rate).map_err(|_| error!(crate::error::ErrorCode::MathOverflow))
}

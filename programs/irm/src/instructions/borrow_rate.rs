use anchor_lang::prelude::*;
use irm_state::IrmState;

#[derive(Accounts)]
pub struct BorrowRate<'info> {
    #[account(
        seeds = [b"irm_config", pool.key().as_ref()],
        bump = irm_state.load()?.bump,
        has_one = pool,
    )]
    pub irm_state: AccountLoader<'info, IrmState>,
    /// CHECK: verified via has_one constraint on irm_state
    pub pool: UncheckedAccount<'info>,
}

pub(crate) fn handler(ctx: Context<BorrowRate>, utilization_bps: u64) -> Result<u32> {
    let config = ctx.accounts.irm_state.load()?;
    let rate = config.model.get_fee_bps(utilization_bps);
    msg!(
        "irm::borrow_rate utilization={} rate={}",
        utilization_bps,
        rate
    );
    Ok(rate)
}

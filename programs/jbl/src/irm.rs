use crate::state::Pool;
use anchor_lang::prelude::*;

pub fn fetch_irm_rate<'a>(pool: &Pool, pool_account: AccountInfo<'a>, irm_state: AccountInfo<'a>) -> Result<u32> {
    let cpi_ctx = CpiContext::new(
        pool.rate_program,
        irm::cpi::accounts::BorrowRate {
            irm_state,
            pool: pool_account,
        },
    );
    Ok(irm::cpi::borrow_rate(cpi_ctx, pool.calculate_utilization())?.get())
}

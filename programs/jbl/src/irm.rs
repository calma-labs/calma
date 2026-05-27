use crate::state::Pool;
use anchor_lang::prelude::*;

/// Fetches the current borrow rate in basis points from the IRM program via CPI.
///
/// The IRM program and its state account must be provided in `remaining_accounts`
/// at indices 0 and 1, matching `pool.rate_program` and `pool.rate_state`.
pub fn fetch_irm_rate<'a>(pool: &Pool, pool_account: AccountInfo<'a>, remaining_accounts: &[AccountInfo<'a>]) -> Result<u32> {
    require!(
        remaining_accounts.len() >= 2,
        crate::error::ErrorCode::MissingRateProgram
    );
    require_keys_eq!(
        remaining_accounts[0].key(),
        pool.rate_program,
        crate::error::ErrorCode::MissingRateProgram
    );
    require_keys_eq!(
        remaining_accounts[1].key(),
        pool.rate_state,
        crate::error::ErrorCode::MissingRateState
    );
    let cpi_ctx = CpiContext::new(
        pool.rate_program,
        irm::cpi::accounts::BorrowRate {
            irm_state: remaining_accounts[1].clone(),
            pool: pool_account,
        },
    );
    Ok(irm::cpi::borrow_rate(cpi_ctx, pool.calculate_utilization())?.get())
}

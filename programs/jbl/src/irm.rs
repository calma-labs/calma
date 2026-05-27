use crate::state::Pool;
use anchor_lang::prelude::*;

pub fn fetch_irm_rate<'a>(pool: &Pool, pool_account: AccountInfo<'a>, remaining_accounts: &[AccountInfo<'a>]) -> Result<Option<u32>> {
    if pool.rate_program == Pubkey::default() {
        return Ok(None);
    }
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
        *remaining_accounts[0].key,
        irm::cpi::accounts::BorrowRate {
            irm_state: remaining_accounts[1].clone(),
            pool: pool_account,
        },
    );
    Ok(Some(
        irm::cpi::borrow_rate(cpi_ctx, pool.calculate_utilization() as u32)?.get(),
    ))
}

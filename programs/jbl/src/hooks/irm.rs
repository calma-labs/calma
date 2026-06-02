use anchor_lang::prelude::*;

/// Proof that a borrow-rate CPI to the IRM program has been executed.
/// Construction performs the CPI; holding an instance is proof it succeeded
/// and `rate_bps` is the verified current borrow rate.
pub struct IrmState {
    pub rate_bps: u32,
    pub current_ts: i64,
}

impl IrmState {
    pub fn new<'a>(
        rate_program: AccountInfo<'a>,
        utilization: u64,
        pool_account: AccountInfo<'a>,
        irm_state: AccountInfo<'a>,
    ) -> Result<Self> {
        let cpi_ctx = CpiContext::new(
            rate_program.key(),
            irm::cpi::accounts::BorrowRate {
                irm_state,
                pool: pool_account,
            },
        );
        let rate_bps = irm::cpi::borrow_rate(cpi_ctx, utilization)?.get();
        let current_ts = Clock::get()?.unix_timestamp;
        Ok(Self { rate_bps, current_ts })
    }
}

impl jbl_math::IrmRate for IrmState {
    fn rate_bps(&self) -> u32 {
        self.rate_bps
    }
    fn current_ts(&self) -> i64 {
        self.current_ts
    }
}

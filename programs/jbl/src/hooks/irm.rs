use anchor_lang::prelude::*;

/// Proof that a borrow-rate CPI to the IRM program has been executed.
/// Construction performs the CPI; holding an instance is proof it succeeded
/// and `rate_bps` is the verified current borrow rate.
pub struct IrmState {
    pub rate_bps: u32,
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
        Ok(Self { rate_bps })
    }
}

impl jbl_math::IrmRate for IrmState {
    fn rate_bps(&self) -> u32 {
        self.rate_bps
    }
}

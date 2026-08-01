pub trait Market {
    fn total_supply_assets(&self) -> u64;
    fn total_supply_shares(&self) -> u64;
    fn total_borrow_assets(&self) -> u64;
    fn total_borrow_shares(&self) -> u64;
    fn last_update(&self) -> i64;
    /// Protocol fee **rate** in basis points, skimmed from accrued interest.
    fn fee(&self) -> u64;
    /// Protocol-owned supply shares accrued from the fee, not yet claimed as LP.
    fn accrued_fee_shares(&self) -> u64;
    fn assets_in_queue(&self) -> u64;
    fn ltv_percent(&self) -> u8;
    /// Principal of the flash loan currently in flight, or 0 when none is.
    /// Lives on the market (not the account wrapper) so `Core` owns the whole
    /// borrow/repay state machine and the client can replay it.
    fn flash_loan_outstanding(&self) -> u64;
    fn total_supply_assets_mut(&mut self) -> &mut u64;
    fn total_supply_shares_mut(&mut self) -> &mut u64;
    fn accrued_fee_shares_mut(&mut self) -> &mut u64;
    fn assets_in_queue_mut(&mut self) -> &mut u64;
    fn total_borrow_assets_mut(&mut self) -> &mut u64;
    fn total_borrow_shares_mut(&mut self) -> &mut u64;
    fn last_update_mut(&mut self) -> &mut i64;
    fn flash_loan_outstanding_mut(&mut self) -> &mut u64;
}

pub trait Position {
    fn collateral_deposited(&self) -> u64;
    fn debt_shares(&self) -> u64;
    fn collateral_deposited_mut(&mut self) -> &mut u64;
    fn debt_shares_mut(&mut self) -> &mut u64;
}

pub trait Clock {
    fn current_ts(&self) -> i64;
}

pub trait IrmRate {
    fn rate_bps(&self) -> u32;
    fn current_ts(&self) -> i64;
}

pub trait Oracle {
    fn price(&self) -> u64;
}

/// Any IRM implementation that can compute a borrow rate from utilization.
pub trait FeeModel {
    fn fee_bps(&self, utilization_bps: u64) -> u32;
}

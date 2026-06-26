pub trait Market {
    fn total_supply_assets(&self) -> u64;
    fn total_supply_shares(&self) -> u64;
    fn total_borrow_assets(&self) -> u64;
    fn total_borrow_shares(&self) -> u64;
    fn last_update(&self) -> i64;
    fn fee(&self) -> u64;
    fn assets_in_queue(&self) -> u64;
    fn ltv_percent(&self) -> u8;
    fn total_supply_assets_mut(&mut self) -> &mut u64;
    fn total_supply_shares_mut(&mut self) -> &mut u64;
    fn assets_in_queue_mut(&mut self) -> &mut u64;
    fn total_borrow_assets_mut(&mut self) -> &mut u64;
    fn total_borrow_shares_mut(&mut self) -> &mut u64;
    fn last_update_mut(&mut self) -> &mut i64;
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

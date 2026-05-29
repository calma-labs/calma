pub trait Market {
    fn total_supply_assets(&self) -> u64;
    fn total_supply_shares(&self) -> u64;
    fn total_borrow_assets(&self) -> u64;
    fn total_borrow_shares(&self) -> u64;
    fn last_update(&self) -> i64;
    fn fee(&self) -> u64;
    fn assets_in_queue(&self) -> u64;
    fn ltv_percent(&self) -> u8;
    fn set_total_supply_assets(&mut self, value: u64);
    fn set_total_supply_shares(&mut self, value: u64);
    fn set_assets_in_queue(&mut self, value: u64);
    fn set_total_borrow_assets(&mut self, value: u64);
    fn set_total_borrow_shares(&mut self, value: u64);
    fn set_last_update(&mut self, value: i64);
}

pub trait Position {
    fn collateral_deposited(&self) -> u64;
    fn debt_shares(&self) -> u64;
    fn set_collateral_deposited(&mut self, value: u64);
    fn set_debt_shares(&mut self, value: u64);
}

pub trait IrmRate {
    fn rate_bps(&self) -> u32;
}

pub trait Oracle {
    fn current_ts(&self) -> i64;
}

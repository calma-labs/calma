pub trait Market {
    fn total_supply_assets(&self) -> u64;
    fn total_supply_shares(&self) -> u64;
    fn total_borrow_assets(&self) -> u64;
    fn total_borrow_shares(&self) -> u64;
    fn last_update(&self) -> i64;
    fn fee(&self) -> u64;
    fn assets_in_queue(&self) -> u64;
    fn ltv_percent(&self) -> u8;
}

pub trait IrmRate {
    fn rate_bps(&self) -> u32;
}

pub trait Oracle {
    fn current_ts(&self) -> i64;
}
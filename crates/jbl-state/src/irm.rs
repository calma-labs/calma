/// Proof that a borrow-rate CPI has been executed. Any type implementing this
/// trait was produced by successfully calling the IRM program; holding an
/// instance is proof the call succeeded and a rate is available.
pub trait IrmRate {
    fn rate_bps(&self) -> u32;
}

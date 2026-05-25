pub mod hedge_offer;
pub mod pool;
pub mod user_position;

pub use hedge_offer::*;
pub use pool::*;
pub use user_position::*;

#[cfg(test)]
mod size_tests {
    use super::*;
    use std::mem::size_of;

    /// Space values hardcoded in instruction `init` constraints must match the
    /// actual struct size (discriminator excluded — Anchor adds 8 bytes on top).
    #[test]
    fn pool_size() {
        // 4 Pubkeys (128) + 6 u64/i64 (48) + UtilizationFeeConfig (32) +
        // ltv_percent + lp_mint_bump (2) + _pad (6) + WithdrawalQueue
        // Full value checked against POOL_SPACE constant in test utils (41 184).
        assert_eq!(size_of::<Pool>(), 41_184);
    }

    #[test]
    fn user_position_size() {
        // authority(32) + pool(32) + collateral_deposited(8) + debt_shares(8) + bump(1) + _pad(7) = 88
        assert_eq!(size_of::<UserPosition>(), 88);
    }

    #[test]
    fn rate_hedge_offer_size() {
        // pool(32) + authority(32) + amount(8) + fixed_rate_bps(8) +
        // min_duration(8) + max_duration(8) + collateral_deposited(8) +
        // locked_tokens(8) + bump(1) + _pad(7) = 120
        assert_eq!(size_of::<RateHedgeOffer>(), 120);
    }

    #[test]
    fn rate_hedge_match_size() {
        // offer(32) + user_position(32) + amount(8) + upfront_fee(8) +
        // initial_debt_shares(8) + start_ts(8) + duration(8) + bump(1) + _pad(7) = 112
        assert_eq!(size_of::<RateHedgeMatch>(), 112);
    }
}

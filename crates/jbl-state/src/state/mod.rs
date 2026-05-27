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
        assert_eq!(size_of::<Pool>(), 41_232);
    }

    #[test]
    fn user_position_size() {
        assert_eq!(size_of::<UserPosition>(), 88);
    }

    #[test]
    fn rate_hedge_offer_size() {
        assert_eq!(size_of::<RateHedgeOffer>(), 120);
    }

    #[test]
    fn rate_hedge_match_size() {
        assert_eq!(size_of::<RateHedgeMatch>(), 112);
    }
}

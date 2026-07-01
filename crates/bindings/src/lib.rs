pub mod exports;
pub mod state;

use wasm_bindgen::prelude::*;

pub(crate) struct BrowserClock;

impl math::Clock for BrowserClock {
    fn current_ts(&self) -> i64 {
        (js_sys::Date::now() / 1000.0) as i64
    }
}

/// Flash-loan fee owed on `amount` at the protocol flash-fee rate.
/// Mirrors the on-chain `flash_borrow`/`flash_repay` fee exactly.
#[wasm_bindgen]
pub fn flash_fee(amount: u64) -> Option<u64> {
    math::flash_fee(amount)
}

/// Evaluate a two-segment linear IRM model at `utilization_bps` without a pool account.
/// Used by the admin IRM curve preview chart.
/// `m1`/`c1` = slope/intercept for segment 0, `m2`/`c2` for segment 1 (all in bps).
#[wasm_bindgen]
pub fn irm_rate_bps(m1: i64, c1: i64, m2: i64, c2: i64, utilization_bps: u64) -> u32 {
    let model = irm_state::PiecewiseLinearModel {
        curves: [
            irm_state::LinearSegment { a: m1, b: c1, enabled: 1, ..Default::default() },
            irm_state::LinearSegment { a: m2, b: c2, enabled: 1, ..Default::default() },
            Default::default(),
            Default::default(),
        ],
    };
    model.get_fee_bps(utilization_bps)
}


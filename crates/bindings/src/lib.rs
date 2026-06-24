pub mod exports;
pub mod state;

use wasm_bindgen::prelude::*;

/// Flash-loan fee owed on `amount` at the protocol flash-fee rate.
/// Mirrors the on-chain `flash_borrow`/`flash_repay` fee exactly.
#[wasm_bindgen]
pub fn flash_fee(amount: u64) -> Option<u64> {
    math::flash_fee(amount)
}

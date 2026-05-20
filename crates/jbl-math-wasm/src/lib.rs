use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compute_interest(total_borrowed: u64, rate_bps: u32, elapsed_secs: u64) -> Option<u64> {
    jbl_math::compute_interest(total_borrowed, rate_bps, elapsed_secs)
}

#[wasm_bindgen]
pub fn amount_to_shares(amount: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    jbl_math::amount_to_shares(amount, total_borrowed, total_debt_shares)
}

#[wasm_bindgen]
pub fn shares_to_amount(shares: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    jbl_math::shares_to_amount(shares, total_borrowed, total_debt_shares)
}

#[wasm_bindgen]
pub fn amount_to_shares_burned(
    repay_amount: u64,
    total_borrowed: u64,
    total_debt_shares: u64,
    max_shares: u64,
) -> Option<u64> {
    jbl_math::amount_to_shares_burned(repay_amount, total_borrowed, total_debt_shares, max_shares)
}

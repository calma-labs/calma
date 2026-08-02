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

// ── on-chain constants ───────────────────────────────────────────────────────
//
// Every number below is defined in Rust and read from there by the browser.
// None may be transcribed into TypeScript, however obvious: a copy is a value
// that can drift, and drift here is silent. `POOL_SPACE` had already drifted by
// 248 bytes when this was added — the app was allocating pool accounts too small
// to create, and nothing failed until a user tried.
//
// wasm-bindgen cannot export a `const`, so each is a zero-argument function.

/// Suggested `price_ttl_ms` when creating a feed. See
/// `interface::DEFAULT_PRICE_TTL_MS`.
#[wasm_bindgen]
pub fn default_price_ttl_ms() -> u32 {
    interface::DEFAULT_PRICE_TTL_MS
}

/// Suggested `rules.max_age_ms` when creating a Pyth feed — the *ingestion*
/// gate, distinct from the TTL above. See `feed_state::DEFAULT_PYTH_MAX_AGE_MS`.
#[wasm_bindgen]
pub fn default_pyth_max_age_ms() -> u32 {
    feed_state::DEFAULT_PYTH_MAX_AGE_MS
}

/// Fixed-point scale for oracle prices and the borrow math's price ratio.
#[wasm_bindgen]
pub fn price_scale() -> u64 {
    interface::PRICE_SCALE as u64
}

/// Bytes to allocate for a `Pool` account, discriminator included.
///
/// Derived from the struct, so it tracks any field added to `Pool` — including
/// ones consumed from `_reserved`, which change nothing here but would be easy
/// to assume did.
#[wasm_bindgen]
pub fn pool_space() -> u32 {
    (8 + core::mem::size_of::<::state::Pool>()) as u32
}

/// Offset of a `Feed` account's `collateral_mint`, for an RPC `memcmp` filter.
#[wasm_bindgen]
pub fn feed_collateral_mint_offset() -> u32 {
    feed_state::FEED_COLLATERAL_MINT_OFFSET
}

/// Offset of a `Feed` account's `lend_mint`, for an RPC `memcmp` filter.
#[wasm_bindgen]
pub fn feed_lend_mint_offset() -> u32 {
    feed_state::FEED_LEND_MINT_OFFSET
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{prelude::Pubkey, AnchorSerialize, Discriminator};

    /// The memcmp offsets are hand-written numbers describing a layout that can
    /// move — `Feed` was restructured around `PriceFeedHeader` once already.
    /// Serialize a real account and find the mints in it.
    #[test]
    fn feed_memcmp_offsets_match_the_serialized_layout() {
        let collateral = Pubkey::new_from_array([3u8; 32]);
        let lend = Pubkey::new_from_array([9u8; 32]);
        let feed = feed_state::Feed {
            header: interface::PriceFeedHeader {
                collateral_mint: collateral,
                lend_mint: lend,
                collateral_price: 1,
                lend_price: 1,
                collateral_decimals: 6,
                lend_decimals: 6,
                last_updated_ts: 0,
                price_ttl_ms: 1,
            },
            id: 0,
            config: feed_state::FeedConfig {
                authority: Pubkey::default(),
                source: feed_state::PriceSource::Manual,
                bump: 0,
                _pad: [0; 6],
                collateral_feed_id: [0; 32],
                lend_feed_id: [0; 32],
            },
            rules: feed_state::FeedRules::default(),
        };

        let mut bytes = feed_state::Feed::DISCRIMINATOR.to_vec();
        feed.serialize(&mut bytes).unwrap();

        let at = |off: u32| &bytes[off as usize..off as usize + 32];
        assert_eq!(at(feed_collateral_mint_offset()), collateral.as_ref());
        assert_eq!(at(feed_lend_mint_offset()), lend.as_ref());
    }
}


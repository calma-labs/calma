pub mod exports;
pub mod state;

use wasm_bindgen::prelude::*;

/// Byte offset of `unix_timestamp` within the Clock sysvar account.
///
/// The sysvar is bincode-encoded — not an Anchor account, so there is no 8-byte
/// discriminator to skip — and its five fields are all 8 bytes, little-endian,
/// in declaration order:
///
/// ```text
///  0  slot: u64
///  8  epoch_start_timestamp: i64
/// 16  epoch: u64
/// 24  leader_schedule_epoch: u64
/// 32  unix_timestamp: i64
/// ```
const CLOCK_UNIX_TIMESTAMP_OFFSET: usize = 32;

/// Total encoded size of the Clock sysvar.
const CLOCK_LEN: usize = 40;

/// Read `Clock::unix_timestamp` out of raw Clock sysvar account data.
///
/// This is the timestamp the on-chain program sees from `Clock::get()`, and the
/// value [`state::PoolWithIrm::from_bytes`] must be given so its accrual
/// replays predict what the program will compute.
///
/// Lives here rather than in TypeScript so the offset is never transcribed —
/// the same reason `POOL_SPACE` and the feed memcmp offsets do. Returns `None`
/// rather than panicking if the slice is not a Clock account.
#[wasm_bindgen]
pub fn clock_unix_timestamp(sysvar_data: &[u8]) -> Option<i64> {
    if sysvar_data.len() < CLOCK_LEN {
        return None;
    }
    let bytes: [u8; 8] = sysvar_data
        [CLOCK_UNIX_TIMESTAMP_OFFSET..CLOCK_UNIX_TIMESTAMP_OFFSET + 8]
        .try_into()
        .ok()?;
    Some(i64::from_le_bytes(bytes))
}

/// Address of the Clock sysvar, so the app does not hardcode the base-58 string.
#[wasm_bindgen]
pub fn clock_sysvar_address() -> String {
    solana_sdk_ids::sysvar::clock::ID.to_string()
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

/// Utilizations (bps) of the rate curve a client should propose by default.
///
/// Split into two parallel arrays because wasm-bindgen cannot return a slice of
/// tuples — the same shape `RatePointsAccount::from_arrays` already takes, so the
/// two compose directly. See `irm_state::DEFAULT_RATE_POINTS`.
#[wasm_bindgen]
pub fn default_rate_point_utils() -> Vec<u16> {
    irm_state::DEFAULT_RATE_POINTS.iter().map(|p| p.0).collect()
}

/// Rates (bps) of the rate curve a client should propose by default, index-aligned
/// with [`default_rate_point_utils`].
#[wasm_bindgen]
pub fn default_rate_point_rates() -> Vec<u32> {
    irm_state::DEFAULT_RATE_POINTS.iter().map(|p| p.1).collect()
}

// ── PDA seed prefixes ────────────────────────────────────────────────────────
//
// A client that derives a program address has to agree with the program on these
// bytes exactly. They were previously written out as string literals at every
// `findProgramAddressSync` call — three places in `app/` and again in
// `packages/test/` — where a rename on-chain produced an address that does not
// exist, and nothing failed until a transaction was actually sent.
//
// Returned as `String` because every seed here is valid UTF-8 and the JS side
// wants `Buffer.from(seed)`; the Rust constants are the `&[u8]` the programs use.

/// Seed prefix of the protocol's signing authority PDA. See `state::seeds::STATE`.
#[wasm_bindgen]
pub fn state_seed() -> String {
    String::from_utf8_lossy(::state::seeds::STATE).into_owned()
}

/// Seed prefix of a market's collateral vault.
#[wasm_bindgen]
pub fn collateral_vault_seed() -> String {
    String::from_utf8_lossy(::state::seeds::COLLATERAL_VAULT).into_owned()
}

/// Seed prefix of a market's lend vault.
#[wasm_bindgen]
pub fn lend_vault_seed() -> String {
    String::from_utf8_lossy(::state::seeds::LEND_VAULT).into_owned()
}

/// Seed prefix of a market's LP mint.
#[wasm_bindgen]
pub fn lp_mint_seed() -> String {
    String::from_utf8_lossy(::state::seeds::LP_MINT).into_owned()
}

/// Seed prefix of a borrower's position account.
#[wasm_bindgen]
pub fn user_position_seed() -> String {
    String::from_utf8_lossy(::state::seeds::USER_POSITION).into_owned()
}

/// Seed prefix of a market's rate-curve account. Part of the rate-provider
/// contract — see `irm_state::IRM_CONFIG_SEED`.
#[wasm_bindgen]
pub fn irm_config_seed() -> String {
    String::from_utf8_lossy(irm_state::IRM_CONFIG_SEED).into_owned()
}

/// Seed prefix of a feed account. See `feed_state::FEED_SEED`.
#[wasm_bindgen]
pub fn feed_seed() -> String {
    String::from_utf8_lossy(feed_state::FEED_SEED).into_owned()
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

    /// The default curve crosses to the browser as two parallel arrays, which is
    /// a shape that can lose an element without failing to compile. Rebuild the
    /// curve from what is actually exported and check the chain would take it.
    #[test]
    fn the_exported_default_curve_survives_the_split_into_arrays() {
        let utils = default_rate_point_utils();
        let rates = default_rate_point_rates();
        assert_eq!(utils.len(), rates.len());
        assert!(crate::state::RatePointsAccount::from_arrays(utils.clone(), rates.clone()).is_some());

        let pairs: Vec<(u16, u32)> = utils.into_iter().zip(rates).collect();
        assert_eq!(pairs.as_slice(), irm_state::DEFAULT_RATE_POINTS.as_slice());
        assert_eq!(irm_state::validate_rate_points(&pairs), Ok(()));
    }

    /// The Clock offset describes a layout, so it gets a test that exercises the
    /// layout rather than restating the number. Every field is 8 bytes, which is
    /// exactly the shape where reading the wrong one still parses cleanly and
    /// silently returns a plausible timestamp.
    #[test]
    fn clock_decoder_picks_unix_timestamp_and_not_a_neighbouring_field() {
        let slot: u64 = 111;
        let epoch_start: i64 = 222;
        let epoch: u64 = 333;
        let leader_schedule_epoch: u64 = 444;
        let unix_timestamp: i64 = 1_700_000_000;

        let mut buf = Vec::new();
        buf.extend_from_slice(&slot.to_le_bytes());
        buf.extend_from_slice(&epoch_start.to_le_bytes());
        buf.extend_from_slice(&epoch.to_le_bytes());
        buf.extend_from_slice(&leader_schedule_epoch.to_le_bytes());
        buf.extend_from_slice(&unix_timestamp.to_le_bytes());
        assert_eq!(buf.len(), super::CLOCK_LEN);

        assert_eq!(clock_unix_timestamp(&buf), Some(unix_timestamp));
        // A short or empty account is not a Clock — refuse rather than guess.
        assert_eq!(clock_unix_timestamp(&buf[..super::CLOCK_LEN - 1]), None);
        assert_eq!(clock_unix_timestamp(&[]), None);
    }

    /// The sysvar address is exported so the app never hardcodes it.
    #[test]
    fn clock_sysvar_address_is_the_canonical_one() {
        assert_eq!(
            clock_sysvar_address(),
            "SysvarC1ock11111111111111111111111111111111"
        );
    }
}


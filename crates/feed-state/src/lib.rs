use anchor_lang::prelude::*;

// The feed types are bound to this program — declare the same ID so the
// #[account] macro can resolve `ID` in the crate root.
declare_id!("orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4");

pub use interface::{price_stale_at, PriceFeedHeader, DEFAULT_PRICE_TTL_MS, PRICE_SCALE};

/// Suggested [`FeedRules::max_age_ms`] for a Pyth-backed feed: 60 seconds.
///
/// The *ingestion* budget — how old an upstream price may be to be written —
/// and necessarily tighter than [`DEFAULT_PRICE_TTL_MS`], the consumption
/// budget. A price has to stay usable for a while after it is accepted, or every
/// borrow would need its own fresh Pyth update.
pub const DEFAULT_PYTH_MAX_AGE_MS: u32 = 60_000;

/// Byte offsets of the two mints within a `Feed` account, discriminator
/// included — what an RPC `memcmp` filter needs to find every feed for a pair.
///
/// Derived from the layout rather than written down twice: the header is the
/// account's first field, and the two mints are its first two.
pub const FEED_COLLATERAL_MINT_OFFSET: u32 = 8;
pub const FEED_LEND_MINT_OFFSET: u32 = FEED_COLLATERAL_MINT_OFFSET + 32;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PriceSource {
    Manual = 0,
    Pyth = 1,
    PythPush = 2,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedConfig {
    pub authority: Pubkey,
    pub source: PriceSource,
    pub bump: u8,
    pub _pad: [u8; 6],
    pub collateral_feed_id: [u8; 32],
    pub lend_feed_id: [u8; 32],
}

/// Optional validation gates applied to Pyth updates. `0` is the "disabled"
/// sentinel for every field so a zero-init `FeedRules` reproduces the legacy
/// behavior. Fixed at feed creation; `set_from_pyth` is the only enforcement
/// site — Manual updates ignore rules entirely.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FeedRules {
    /// Reject if `conf / price` exceeds this many basis points.
    pub max_conf_bps: u16,
    /// Reject if `|new − last| / last` exceeds `bps_per_hour × elapsed_hours`,
    /// with the effective ceiling clamped at 10_000 bps.
    pub max_deviation_bps_per_hour: u16,
    /// Reject if `|price − ema_price| / ema_price` exceeds this many bps.
    pub ema_divergence_bps: u16,
    /// Reject if normalized price is below this floor (in PRICE_SCALE units).
    pub min_price: u64,
    /// Reject if normalized price is above this ceiling (in PRICE_SCALE units).
    pub max_price: u64,
    /// Reject if the Pyth price is older than this many milliseconds. Required
    /// (> 0) for Pyth / PythPush feeds; `0` disables the check (Manual feeds).
    ///
    /// An **ingestion** gate — not to be confused with
    /// `PriceFeedHeader::price_ttl_ms`, which bounds how long a consumer may act
    /// on a price already written. Their `0` conventions are opposites; see
    /// `interface::price_stale_at` and the note on `rules::check_max_age`.
    pub max_age_ms: u32,
    pub _reserved: [u8; 4],
}

/// This program's price account.
///
/// The [`PriceFeedHeader`] prefix is the contract `calma` reads, so it must stay
/// first, immediately after the discriminator. Everything below it is this
/// implementation's own business — which Pyth ids to pull from, who may write,
/// which ingestion rules apply. A different oracle program keeps the header and
/// puts something else entirely underneath.
#[account]
#[derive(Copy)]
pub struct Feed {
    pub header: PriceFeedHeader,
    pub id: u8,
    pub config: FeedConfig,
    pub rules: FeedRules,
}

impl Feed {
    /// `true` iff a Pyth price with `publish_time` would fail the *ingestion*
    /// gate when consumed at wall-clock `clock_ts`. `0` in `rules.max_age_ms`
    /// disables the check (returns `false`). Exposed to the browser via the
    /// wasm bindings so the UI can predict `StalePushPrice` before submitting.
    pub fn is_pyth_price_stale(&self, publish_time: i64, clock_ts: i64) -> bool {
        let max_age_ms = self.rules.max_age_ms;
        if max_age_ms == 0 {
            return false;
        }
        let elapsed_ms = clock_ts.saturating_sub(publish_time).saturating_mul(1_000);
        elapsed_ms > max_age_ms as i64
    }
}

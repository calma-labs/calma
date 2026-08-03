use anchor_lang::prelude::*;

// The feed types are bound to this program — declare the same ID so the
// #[account] macro can resolve `ID` in the crate root.
declare_id!("orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4");

pub use interface::{price_stale_at, PriceFeedHeader, DEFAULT_PRICE_TTL_MS, PRICE_SCALE};

/// PDA seed prefix for a feed account: `["feed", collateral_mint, lend_mint, id]`.
///
/// Clients derive feed addresses from this, so it is not free to change without
/// changing them too. Exposed to the browser as `bindings::feed_seed`.
pub const FEED_SEED: &[u8] = b"feed";

/// Floor on a *non-zero* [`FeedRules::max_deviation_bps_per_hour`]: 10% per
/// hour.
///
/// The deviation guard rejects an update that moves faster than its budget
/// allows, rather than clamping it — which is the right behaviour for a guard,
/// and also means a market can be frozen by its own oracle. A rejected update
/// leaves the last price in place; once `price_ttl_ms` elapses, consumers read
/// the feed as stale and every instruction that needs a price starts failing.
/// The budget is time-scaled and uncapped, so it always recovers, but "always"
/// can be many hours if the budget is tight.
///
/// This floor does not eliminate that — no value can, since it is a trade-off
/// between admitting a bad print and refusing a real move — but it puts a bound
/// on it. At 10% per hour a halving is admitted after roughly five hours rather
/// than the days a much tighter setting would need. Anything below this is
/// almost certainly a mistake rather than a risk appetite, so it is refused at
/// creation instead of discovered during a crash.
///
/// `0` remains available and still means "disabled" — an explicit choice to run
/// without the guard, which is the right call for a volatile pair where a freeze
/// is the worse failure.
pub const MIN_DEVIATION_BPS_PER_HOUR: u16 = 1_000;

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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn feed(max_age_ms: u32) -> Feed {
        Feed {
            header: PriceFeedHeader {
                collateral_mint: Pubkey::default(),
                lend_mint: Pubkey::default(),
                collateral_price: 1_000_000,
                lend_price: 1_000_000,
                collateral_decimals: 6,
                lend_decimals: 6,
                last_updated_ts: NOW,
                price_ttl_ms: DEFAULT_PRICE_TTL_MS,
            },
            id: 0,
            config: FeedConfig {
                authority: Pubkey::default(),
                source: PriceSource::Pyth,
                bump: 0,
                _pad: [0; 6],
                collateral_feed_id: [0; 32],
                lend_feed_id: [0; 32],
            },
            rules: FeedRules {
                max_age_ms,
                ..Default::default()
            },
        }
    }

    // ── the two staleness gates disagree about zero, on purpose ──────────────

    /// The single most confusable thing in this crate. `rules.max_age_ms` is an
    /// *ingestion* gate where `0` means "don't check", so a zero-init `FeedRules`
    /// reproduces pre-rules behavior. `header.price_ttl_ms` is a *consumption*
    /// gate where `0` means "reject everything", because the only safe reading of
    /// a missing budget on a borrow gate is to fail closed.
    ///
    /// Both predicates are one line and read almost identically. Pinning them
    /// against each other here means a change that "tidies up" one to match the
    /// other cannot pass silently.
    #[test]
    fn zero_disables_ingestion_but_rejects_consumption() {
        let f = feed(0);
        // Ingestion: an ancient price is accepted, because the check is off.
        assert!(!f.is_pyth_price_stale(NOW - 86_400, NOW));

        // Consumption: a price written this very second is refused.
        let mut header = f.header;
        header.price_ttl_ms = 0;
        assert!(header.is_stale_at(NOW));
        assert!(price_stale_at(NOW, NOW, 0));
    }

    #[test]
    fn ingestion_budget_is_applied_in_milliseconds() {
        let f = feed(60_000);
        // Exactly at the 60 s budget is still fresh.
        assert!(!f.is_pyth_price_stale(NOW - 60, NOW));
        // One second past it is not.
        assert!(f.is_pyth_price_stale(NOW - 61, NOW));
    }

    #[test]
    fn a_price_published_ahead_of_the_clock_is_not_stale() {
        // Publishers can run slightly early; that is not staleness.
        assert!(!feed(60_000).is_pyth_price_stale(NOW + 5, NOW));
    }

    #[test]
    fn extreme_timestamps_saturate_rather_than_overflow() {
        let f = feed(u32::MAX);
        assert!(f.is_pyth_price_stale(i64::MIN, i64::MAX));
        assert!(!f.is_pyth_price_stale(i64::MAX, i64::MIN));
    }

    // ── wire-format constants ────────────────────────────────────────────────

    /// `PriceSource` crosses to the browser as a raw `u8` (`FeedAccount::source`),
    /// and the app switches on the number. Reordering the variants would silently
    /// turn every Pyth feed into a Manual one in the UI.
    #[test]
    fn price_source_discriminants_are_pinned() {
        assert_eq!(PriceSource::Manual as u8, 0);
        assert_eq!(PriceSource::Pyth as u8, 1);
        assert_eq!(PriceSource::PythPush as u8, 2);
    }

    /// The documented contract of `FeedRules`: zero-initialized means every gate
    /// is off. `create_handler` relies on this for Manual feeds.
    #[test]
    fn default_rules_disable_every_gate() {
        let r = FeedRules::default();
        assert_eq!(r.max_conf_bps, 0);
        assert_eq!(r.max_deviation_bps_per_hour, 0);
        assert_eq!(r.ema_divergence_bps, 0);
        assert_eq!(r.min_price, 0);
        assert_eq!(r.max_price, 0);
        assert_eq!(r.max_age_ms, 0);
    }

    /// The header must stay first in `Feed`, immediately after the discriminator
    /// — that prefix is the whole oracle contract. These offsets are also used as
    /// RPC `memcmp` filters, so derive them from a real serialized account rather
    /// than trusting the declaration order.
    #[test]
    fn the_price_feed_header_is_the_first_thing_in_the_account() {
        use anchor_lang::{AnchorSerialize, Discriminator};

        let collateral = Pubkey::new_from_array([7u8; 32]);
        let lend = Pubkey::new_from_array([11u8; 32]);
        let mut f = feed(60_000);
        f.header.collateral_mint = collateral;
        f.header.lend_mint = lend;

        let mut bytes = Feed::DISCRIMINATOR.to_vec();
        f.serialize(&mut bytes).unwrap();

        let at = |off: u32| &bytes[off as usize..off as usize + 32];
        assert_eq!(at(FEED_COLLATERAL_MINT_OFFSET), collateral.as_ref());
        assert_eq!(at(FEED_LEND_MINT_OFFSET), lend.as_ref());
    }
}

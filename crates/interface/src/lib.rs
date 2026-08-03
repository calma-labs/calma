//! The account layouts `calma` exports to the programs it consumes.
//!
//! `calma` used to CPI into one specific oracle program, which pinned the
//! protocol to that program's ID at compile time. The direction is now
//! inverted: `calma` defines the shape a price account must have, reads it
//! directly, and any program that writes accounts in that shape can serve as an
//! oracle for a market.
//!
//! This crate deliberately declares **no program ID**. Binding it to one would
//! reintroduce exactly the coupling it exists to remove — `Account<'info, T>`
//! resolves its owner check from the defining crate's `ID`, which is why
//! consumers here go through [`read_price_feed`] and check the owner against a
//! value recorded per-market instead.

use anchor_lang::prelude::*;

/// PDA seed prefix for a market's rate-curve account: `["irm_config", pool]`.
///
/// Part of the rate-provider contract in the same way [`PriceFeedHeader`] is
/// part of the oracle contract: `calma::create` re-derives this address under
/// whichever program a market names as its rate program and refuses anything
/// else, so a provider that seeded its account differently could never serve a
/// market. `programs/irm` and `programs/quote` both sit at it.
///
/// It lives here, and not in the reference implementation's `irm-state`, for the
/// reason stated at the top of this module: a crate with no program ID is the
/// only honest home for something every provider has to agree on. `calma` cannot
/// reach `irm-state` anyway — that crate carries the reference IRM's
/// `declare_id!`, and the lending program links no provider. Before this moved,
/// `calma` re-typed the literal at the one site where the derivation is
/// security-critical, which is the drift the constant exists to prevent.
pub const IRM_CONFIG_SEED: &[u8] = b"irm_config";

/// Fixed-point scale for prices: a [`PriceFeedHeader::price_ratio`] of
/// `1_000_000` means one unit of collateral is worth one unit of the lend
/// token. Matches `math::PRICE_SCALE`, which the borrow math divides by.
pub const PRICE_SCALE: u128 = 1_000_000;

/// Suggested [`PriceFeedHeader::price_ttl_ms`] for a feed with no reason to
/// choose otherwise: 90 seconds.
///
/// Only a default — the field is the oracle's to set, and a publisher that knows
/// its own cadence should. It lives here, rather than in whichever client
/// happens to create feeds, so every one of them proposes the same number and a
/// change reaches all of them at once. Exposed to the browser through
/// `bindings::default_price_ttl_ms`.
pub const DEFAULT_PRICE_TTL_MS: u32 = 90_000;

/// The prefix every price account must lay out first, immediately after its
/// 8-byte account discriminator.
///
/// An implementation appends whatever else it needs below this — Pyth feed ids,
/// validation rules, governance fields — and consumers ignore the tail. That is
/// what lets a Pyth-backed feed, a manually-set feed and a TWAP feed all serve
/// the same market type without `calma` knowing anything about them.
///
/// Prices are quoted per whole token; `collateral_decimals` / `lend_decimals`
/// let [`price_ratio`](Self::price_ratio) fold the mints' decimal exponents into
/// the ratio, so consumers never apply a decimal adjustment of their own.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceFeedHeader {
    pub collateral_mint: Pubkey,
    pub lend_mint: Pubkey,
    pub collateral_price: u64,
    pub lend_price: u64,
    pub collateral_decimals: u8,
    pub lend_decimals: u8,
    /// When the prices above were last written, in Unix seconds. For a feed
    /// sourced from an upstream publisher this is the *publisher's* timestamp,
    /// not the time the write landed — otherwise relaying a stale price would
    /// launder it into a fresh one.
    pub last_updated_ts: i64,
    /// Maximum age, in milliseconds, at which a consumer may still act on this
    /// price. The oracle owns this: it knows its own update cadence, so it — not
    /// each market — decides how long a quote stays good for.
    ///
    /// `0` **rejects everything**. A missing budget on a gate can only safely
    /// mean "refuse"; see [`price_stale_at`].
    pub price_ttl_ms: u32,
}

impl PriceFeedHeader {
    /// Decimal-adjusted price of the collateral token expressed in lend tokens,
    /// in [`PRICE_SCALE`] units. `None` on overflow or a zero denominator.
    ///
    /// This is the *only* copy of this formula. The feed program writes the two
    /// raw prices; every consumer — `calma`'s borrow gate and the wasm bindings
    /// that predict it in the browser — derives the ratio here, so a client
    /// preview cannot drift from what the chain will compute.
    pub fn price_ratio(&self) -> Option<u64> {
        if self.lend_price == 0 {
            return None;
        }
        let coll_price = self.collateral_price as u128;
        let lend_price = self.lend_price as u128;

        let coll_dec_pow = 10u128.checked_pow(self.collateral_decimals as u32)?;
        let lend_dec_pow = 10u128.checked_pow(self.lend_decimals as u32)?;

        let numerator = coll_price
            .checked_mul(lend_dec_pow)?
            .checked_mul(PRICE_SCALE)?;
        let denominator = lend_price.checked_mul(coll_dec_pow)?;
        if denominator == 0 {
            return None;
        }

        u64::try_from(numerator.checked_div(denominator)?).ok()
    }

    /// `true` iff this price is too old to act on at `clock_ts`, per its own
    /// [`price_ttl_ms`](Self::price_ttl_ms).
    pub fn is_stale_at(&self, clock_ts: i64) -> bool {
        price_stale_at(self.last_updated_ts, clock_ts, self.price_ttl_ms)
    }
}

/// A header with an unusable price reads as a price of `0`, which every
/// `math::Core` operation that consumes an oracle already refuses.
impl math::Oracle for PriceFeedHeader {
    fn price(&self) -> u64 {
        self.price_ratio().unwrap_or(0)
    }
}

/// `true` iff a price stamped `last_updated_ts` is stale when read at
/// wall-clock `clock_ts` under a budget of `ttl_ms`.
///
/// # `ttl_ms == 0` means "reject", not "disabled"
///
/// This is the **opposite** of the `max_age_ms` convention used by feed
/// *ingestion* rules, where `0` is the disabled sentinel shared by every gate so
/// a zero-init rules struct reproduces pre-rules behavior. The two predicates
/// read almost identically, so the divergence is stated here rather than left to
/// be inferred: on the ingestion side `0` opts out of a validation, while this
/// is the gate that guards borrowing, and the only safe reading of a missing
/// budget on a *gate* is to fail closed.
///
/// It also means a price account from before this field existed — whose bytes
/// decode as `0` — refuses to be borrowed against rather than being accepted
/// with no freshness requirement at all.
///
/// Compared in milliseconds so the configured budget applies exactly as
/// written, with no seconds truncation. The underlying timestamps are Unix
/// seconds, so the measured age still moves in whole seconds — the unit buys
/// threshold precision, not clock precision. `i128` keeps the ×1000 total even
/// at extreme timestamps.
pub fn price_stale_at(last_updated_ts: i64, clock_ts: i64, ttl_ms: u32) -> bool {
    if ttl_ms == 0 {
        return true;
    }
    let elapsed_ms = (clock_ts as i128 - last_updated_ts as i128) * 1_000;
    elapsed_ms > ttl_ms as i128
}

/// Read a [`PriceFeedHeader`] out of an arbitrary price account.
///
/// `expected_key` and `expected_owner` are the values the *market* recorded when
/// it was created (`Pool::feed_state` and `Pool::feed_program`). Together they
/// are the whole security model: the address pins which account, and the owner
/// pins which program is allowed to have written it. Neither alone is enough —
/// an address with no owner check accepts an account some other program has
/// since come to own, and an owner check with no address check accepts any of
/// that program's accounts, including one the caller just created and priced
/// themselves.
///
/// The 8-byte discriminator is skipped but **not** verified. With pluggable
/// oracles each implementation names its account struct differently and so
/// carries a different discriminator; there is no single value to compare
/// against. Nothing is lost by it: the address is already pinned to one exact
/// account, so there is no second account of a different type for a
/// discriminator check to catch.
///
/// Bytes past the header are ignored, which is what lets implementations extend
/// the account with their own fields.
pub fn read_price_feed(
    acc: &AccountInfo,
    expected_key: Pubkey,
    expected_owner: Pubkey,
) -> Result<PriceFeedHeader> {
    require_keys_eq!(
        *acc.key,
        expected_key,
        anchor_lang::error::ErrorCode::ConstraintAddress
    );
    require_keys_eq!(
        *acc.owner,
        expected_owner,
        anchor_lang::error::ErrorCode::ConstraintOwner
    );

    let data = acc.try_borrow_data()?;
    let mut cursor = data
        .get(8..)
        .ok_or(anchor_lang::error::ErrorCode::AccountDidNotDeserialize)?;
    PriceFeedHeader::deserialize(&mut cursor)
        .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn header(coll: u64, lend: u64, coll_dec: u8, lend_dec: u8) -> PriceFeedHeader {
        PriceFeedHeader {
            collateral_mint: Pubkey::default(),
            lend_mint: Pubkey::default(),
            collateral_price: coll,
            lend_price: lend,
            collateral_decimals: coll_dec,
            lend_decimals: lend_dec,
            last_updated_ts: NOW,
            price_ttl_ms: 90_000,
        }
    }

    // ── staleness ────────────────────────────────────────────────────────────

    #[test]
    fn zero_budget_fails_closed() {
        // Every case is stale, including the same-second price that would slip
        // through under a "0 disables" reading and the future-dated one that
        // passes under any real budget.
        assert!(price_stale_at(NOW, NOW, 0));
        assert!(price_stale_at(NOW - 1, NOW, 0));
        assert!(price_stale_at(NOW + 5, NOW, 0));
    }

    #[test]
    fn budget_is_applied_in_milliseconds() {
        // 90s elapsed against a 90_000 ms budget: exactly at the limit, fresh.
        assert!(!price_stale_at(NOW - 90, NOW, 90_000));
        // One second past it.
        assert!(price_stale_at(NOW - 91, NOW, 90_000));
        // Sub-second budgets are not truncated to zero: 1_500 ms admits a
        // one-second-old price and refuses a two-second-old one.
        assert!(!price_stale_at(NOW - 1, NOW, 1_500));
        assert!(price_stale_at(NOW - 2, NOW, 1_500));
    }

    #[test]
    fn a_price_published_ahead_of_the_clock_is_fresh() {
        // Publishers can run slightly early; that is not staleness.
        assert!(!price_stale_at(NOW + 5, NOW, 1_000));
    }

    #[test]
    fn extreme_timestamps_stay_total() {
        // The ×1000 is done in i128 so neither bound can overflow or wrap.
        assert!(price_stale_at(i64::MIN, i64::MAX, u32::MAX));
        assert!(!price_stale_at(i64::MAX, i64::MIN, u32::MAX));
    }

    #[test]
    fn is_stale_at_reads_the_headers_own_ttl() {
        let mut h = header(1_000_000, 1_000_000, 6, 6);
        assert!(!h.is_stale_at(NOW + 90));
        assert!(h.is_stale_at(NOW + 91));
        h.price_ttl_ms = 0;
        assert!(h.is_stale_at(NOW));
    }

    // ── ratio ────────────────────────────────────────────────────────────────

    #[test]
    fn equal_prices_and_decimals_give_parity() {
        assert_eq!(
            header(1_000_000, 1_000_000, 6, 6).price_ratio(),
            Some(PRICE_SCALE as u64)
        );
    }

    #[test]
    fn ratio_tracks_the_price_quotient() {
        // Collateral worth 2 lend tokens.
        assert_eq!(
            header(2_000_000, 1_000_000, 6, 6).price_ratio(),
            Some(2 * PRICE_SCALE as u64)
        );
        // ...and worth half of one.
        assert_eq!(
            header(500_000, 1_000_000, 6, 6).price_ratio(),
            Some(PRICE_SCALE as u64 / 2)
        );
    }

    #[test]
    fn decimal_exponents_are_folded_into_the_ratio() {
        // Same per-token price, but collateral has 9 decimals to lend's 6: one
        // base unit of collateral is worth 1/1000 of a lend base unit.
        assert_eq!(
            header(1_000_000, 1_000_000, 9, 6).price_ratio(),
            Some(PRICE_SCALE as u64 / 1_000)
        );
        // The inverse direction scales up by the same factor.
        assert_eq!(
            header(1_000_000, 1_000_000, 6, 9).price_ratio(),
            Some(PRICE_SCALE as u64 * 1_000)
        );
    }

    #[test]
    fn zero_lend_price_is_none_not_a_division_by_zero() {
        assert_eq!(header(1_000_000, 0, 6, 6).price_ratio(), None);
    }

    #[test]
    fn zero_collateral_price_is_a_valid_zero_ratio() {
        // Distinct from the `None` above: the feed is usable, the collateral is
        // simply worthless, and the borrow gate should see a capacity of 0.
        assert_eq!(header(0, 1_000_000, 6, 6).price_ratio(), Some(0));
    }

    #[test]
    fn overflowing_ratio_is_none() {
        // u64::MAX collateral against a 1-unit lend price overflows the u64
        // result after the ×PRICE_SCALE.
        assert_eq!(header(u64::MAX, 1, 6, 6).price_ratio(), None);
        // An absurd decimals value overflows the 10^n exponent itself.
        assert_eq!(header(1_000_000, 1_000_000, u8::MAX, 6).price_ratio(), None);
    }

    #[test]
    fn oracle_impl_surfaces_unusable_prices_as_zero() {
        use math::Oracle;
        assert_eq!(header(1_000_000, 0, 6, 6).price(), 0);
        assert_eq!(header(u64::MAX, 1, 6, 6).price(), 0);
        assert_eq!(header(1_000_000, 1_000_000, 6, 6).price(), PRICE_SCALE as u64);
    }

    // ── wire format ──────────────────────────────────────────────────────────

    #[test]
    fn header_deserializes_from_a_prefix_and_ignores_the_tail() {
        // The property the whole interface rests on: an implementation may
        // append anything below the header and consumers still read it.
        let h = header(3_000_000, 1_500_000, 6, 9);
        let mut buf = Vec::new();
        h.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[0xAB; 64]);

        let mut cursor = &buf[..];
        assert_eq!(PriceFeedHeader::deserialize(&mut cursor).unwrap(), h);
        assert_eq!(cursor.len(), 64, "tail must be left untouched");
    }
}

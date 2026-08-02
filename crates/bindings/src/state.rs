//! Zero-copy deserialization of Anchor account bytes into `state` types.
//!
//! Each `parse_*` function expects the full raw account bytes as transmitted
//! by the server (8-byte Anchor discriminator included).  It strips the
//! discriminator, verifies the remaining length, then casts the bytes directly
//! to the canonical struct type using `bytemuck::pod_read_unaligned`.
//!
//! Both AMD64 (server) and wasm32 (client) are little-endian, so no
//! byte-swapping is required.

use anchor_lang::AccountDeserialize;
use bytemuck::Pod;
use feed_state::Feed;
use irm_state::IrmState;
use math::{Clock, Core};
use state::{Pool, UserPosition};
use wasm_bindgen::prelude::*;

const DISCRIMINATOR: usize = 8;

// ── wrapper types ─────────────────────────────────────────────────────────────

/// Wasm-exposed wrapper around a parsed `Pool` account.
#[wasm_bindgen]
pub struct PoolAccount(pub(crate) Pool);
/// Wasm-exposed wrapper around a parsed `UserPosition` account.
#[wasm_bindgen]
pub struct UserPositionAccount(pub(crate) UserPosition);
/// Wasm-exposed wrapper around a parsed `IrmConfig` account from the irm program.
#[wasm_bindgen]
pub struct IrmConfigAccount(IrmState);
/// Wasm-exposed wrapper around a `PiecewiseLinearModel` — a 2..=4 point
/// utilization → rate curve.  Constructed from parallel `utils` / `rates`
/// arrays; `rate_bps` replays the on-chain evaluator exactly.
#[wasm_bindgen]
pub struct RatePointsAccount(irm_state::PiecewiseLinearModel);
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct FeedAccount(pub(crate) Feed);

// ── parsing ───────────────────────────────────────────────────────────────────

fn parse<T: Pod>(account_data: &[u8]) -> Option<T> {
    let body = account_data.get(DISCRIMINATOR..)?;
    if body.len() != core::mem::size_of::<T>() {
        return None;
    }
    Some(bytemuck::pod_read_unaligned(body))
}

#[wasm_bindgen]
impl PoolAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    pub fn from_bytes(account_data: &[u8]) -> Option<PoolAccount> {
        parse(account_data).map(Self)
    }

    /// Authority pubkey as raw 32 bytes. Use `new PublicKey(pool.authority)` on the JS side.
    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.authority).to_vec()
    }

    /// Collateral mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn collateral_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.collateral_mint).to_vec()
    }

    /// Lend mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn lend_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.lend_mint).to_vec()
    }

    /// LP mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn lp_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.lp_mint).to_vec()
    }

    /// Sum of lend tokens currently deposited.
    #[wasm_bindgen(getter)]
    pub fn total_supply_assets(&self) -> u64 {
        self.0.market.total_supply_assets
    }

    /// Total LP tokens outstanding for the lend side.
    #[wasm_bindgen(getter)]
    pub fn total_supply_shares(&self) -> u64 {
        self.0.market.total_supply_shares
    }

    /// Total borrowed lend tokens outstanding.
    #[wasm_bindgen(getter)]
    pub fn total_borrow_assets(&self) -> u64 {
        self.0.market.total_borrow_assets
    }

    /// Total debt shares outstanding.
    #[wasm_bindgen(getter)]
    pub fn total_borrow_shares(&self) -> u64 {
        self.0.market.total_borrow_shares
    }

    /// Unix timestamp of the last interest accrual.
    #[wasm_bindgen(getter)]
    pub fn last_update(&self) -> i64 {
        self.0.market.last_update
    }

    #[wasm_bindgen(getter)]
    pub fn fee(&self) -> u64 {
        self.0.market.fee
    }

    #[wasm_bindgen(getter)]
    pub fn assets_in_queue(&self) -> u64 {
        self.0.market.assets_in_queue
    }

    /// LTV percent (e.g. 80 means 80%).
    #[wasm_bindgen(getter)]
    pub fn ltv_percent(&self) -> u8 {
        self.0.market.ltv_percent
    }

    /// LP mint PDA bump seed.
    #[wasm_bindgen(getter)]
    pub fn lp_mint_bump(&self) -> u8 {
        self.0.lp_mint_bump
    }

    /// IRM state (IRM config) pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn irm_state(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.irm_state).to_vec()
    }

    /// Price account this market reads, as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn feed_state(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.feed_state).to_vec()
    }

    /// Whitelist this market is gated on, as raw 32 bytes — all zeroes for an
    /// open market (see [`Self::is_guarded`]). Entry instructions
    /// (`deposit_collateral`, `borrow`, `deposit_lent`) must be sent with
    /// `guard_program` + this account whenever it is set, or they fail
    /// `GuardRequired`.
    #[wasm_bindgen(getter)]
    pub fn guard_state(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.guard_state).to_vec()
    }

    /// `true` when entering this market requires being on its whitelist.
    pub fn is_guarded(&self) -> bool {
        self.0.guard_state != anchor_lang::prelude::Pubkey::default()
    }

    // ── which programs this market trusts ────────────────────────────────────
    //
    // None of these are protocol constants. `calma` links no provider, so each
    // market names its own oracle, rate model and whitelist at creation and is
    // pinned to them for life. That makes the three pubkeys below the trust
    // decision a depositor is actually taking on — and the reason they are
    // exposed at all: a market whose price or rate comes from a program nobody
    // recognises is a fact the UI should be able to surface, not one buried in
    // an account nobody decodes.

    /// Program that owns [`Self::feed_state`] and is trusted to price this
    /// market. Any program writing an `interface::PriceFeedHeader` may serve.
    #[wasm_bindgen(getter)]
    pub fn feed_program(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.feed_program).to_vec()
    }

    /// Program CPI'd for this market's borrow rate. Any program implementing
    /// `borrow_rate` / `check_authority` may serve.
    #[wasm_bindgen(getter)]
    pub fn rate_program(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.rate_program).to_vec()
    }

    /// Program asked to vouch for [`Self::guard_state`], or all zeroes on an
    /// open market. Pinned alongside the whitelist: substituting either is
    /// refused, so this names who actually decides admission.
    #[wasm_bindgen(getter)]
    pub fn guard_program(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.guard_program).to_vec()
    }

    /// Total lend tokens committed to pending withdrawals in the on-chain queue.
    pub fn pending_withdrawals(&self) -> u64 {
        self.0.market.assets_in_queue
    }

    /// Effective utilization in basis points (0..10_000), including pending withdrawals.
    /// Uses the shared `math::utilization_bps` (same formula as the on-chain
    /// `Pool::calculate_utilization`), capped at 10_000 for display.
    pub fn utilization_bps(&self) -> u16 {
        let m = &self.0.market;
        math::utilization_bps(m.total_supply_assets, m.total_borrow_assets, m.assets_in_queue)
            .min(10_000) as u16
    }

    /// Available liquidity in raw token units (lend deposited minus effective borrowed).
    pub fn available_liquidity(&self) -> u64 {
        let effective_borrowed = self
            .0
            .market
            .total_borrow_assets
            .saturating_add(self.pending_withdrawals());
        self.0
            .market
            .total_supply_assets
            .saturating_sub(effective_borrowed)
    }
}

#[wasm_bindgen]
impl UserPositionAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    pub fn from_bytes(account_data: &[u8]) -> Option<UserPositionAccount> {
        parse(account_data).map(Self)
    }

    /// Authority pubkey as raw 32 bytes (`Uint8Array` in JS).
    /// Use `new PublicKey(pos.authority)` on the JS side to reconstruct.
    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.authority).to_vec()
    }

    /// Pool pubkey as raw 32 bytes (`Uint8Array` in JS).
    /// Use `new PublicKey(pos.pool)` on the JS side to reconstruct.
    #[wasm_bindgen(getter)]
    pub fn pool(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.pool).to_vec()
    }

    /// Raw collateral deposited in token base units.
    #[wasm_bindgen(getter)]
    pub fn collateral_deposited(&self) -> u64 {
        self.0.collateral_deposited
    }

    /// Raw debt shares held by this position.
    #[wasm_bindgen(getter)]
    pub fn debt_shares(&self) -> u64 {
        self.0.debt_shares
    }

    /// PDA bump seed.
    #[wasm_bindgen(getter)]
    pub fn bump(&self) -> u8 {
        self.0.bump
    }

    /// Returns `true` if collateral has been deposited.
    pub fn has_collateral(&self) -> bool {
        self.0.collateral_deposited > 0
    }

    /// Returns `true` if there is an active borrow (debt shares > 0).
    pub fn has_debt(&self) -> bool {
        self.0.debt_shares > 0
    }

    /// Human-readable collateral amount as a decimal string (e.g. `"1234.5678"`).
    /// Trailing zeros are trimmed; at most 6 decimal places are shown.
    pub fn format_collateral(&self, decimals: u8) -> String {
        format_token_capped(self.0.collateral_deposited, decimals)
    }

    // NOTE: debt-derived metrics (debt amount, LTV, health factor, liquidation
    // price, formatted debt) require interest accrual, so they live on
    // `PoolWithIrm` (which carries the IRM) and are computed by replaying `Core`
    // — see `PoolWithIrm::debt_amount`, `ltv`, `health_factor`, `liq_price`,
    // `max_borrowable`, `format_debt`.
}

#[wasm_bindgen]
impl RatePointsAccount {
    /// Build from parallel `utils` (u16, 0..=10_000) and `rates` (u32 bps) arrays.
    ///
    /// Validates the on-chain invariants once, so `rate_bps` afterwards performs
    /// zero checks (hot path — CU/perf matters).
    ///
    /// Returns `None` if any invariant fails:
    ///   * `utils.len() == rates.len()`
    ///   * length in `2..=4`
    ///   * `utils[0] == 0`
    ///   * `utils` strictly increasing
    pub fn from_arrays(utils: Vec<u16>, rates: Vec<u32>) -> Option<RatePointsAccount> {
        if utils.len() != rates.len() {
            return None;
        }
        let len = utils.len();
        if !(irm_state::MIN_POINTS..=irm_state::MAX_POINTS).contains(&len) {
            return None;
        }
        if utils[0] != 0 {
            return None;
        }
        for i in 1..len {
            if utils[i] <= utils[i - 1] {
                return None;
            }
        }

        let mut points = [irm_state::RatePoint::default(); irm_state::MAX_POINTS];
        for i in 0..len {
            points[i] = irm_state::RatePoint::new(utils[i], rates[i]);
        }
        Some(RatePointsAccount(irm_state::PiecewiseLinearModel {
            points,
            len: len as u8,
            _pad: [0; 7],
        }))
    }

    /// Effective borrow rate in basis points at `utilization_bps`.
    /// Delegates to `PiecewiseLinearModel::get_fee_bps` — no client-side math.
    pub fn rate_bps(&self, utilization_bps: u64) -> u32 {
        self.0.get_fee_bps(utilization_bps)
    }

    #[wasm_bindgen(getter)]
    pub fn len(&self) -> u8 {
        self.0.len
    }

    /// Utilization (bps) of the point at index `i`. Returns 0 if `i >= len`.
    pub fn util_at(&self, i: u8) -> u16 {
        let idx = i as usize;
        if idx >= self.0.len as usize {
            0
        } else {
            self.0.points[idx].util_bps
        }
    }

    /// Rate (bps) of the point at index `i`. Returns 0 if `i >= len`.
    pub fn rate_at(&self, i: u8) -> u32 {
        let idx = i as usize;
        if idx >= self.0.len as usize {
            0
        } else {
            self.0.points[idx].rate_bps
        }
    }
}

#[wasm_bindgen]
impl IrmConfigAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    pub fn from_bytes(account_data: &[u8]) -> Option<IrmConfigAccount> {
        parse(account_data).map(Self)
    }

    /// Pool pubkey this IRM config is bound to, as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn pool(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.pool).to_vec()
    }

    /// Effective borrow rate in basis points for a given utilization (0..10_000).
    pub fn fee_bps(&self, utilization_bps: u64) -> u32 {
        self.0.model.get_fee_bps(utilization_bps)
    }

    /// Number of live rate points (2..=4).
    pub fn points_len(&self) -> u8 {
        self.0.model.len
    }

    /// Utilization (bps) of the point at index `i`. Returns 0 if `i >= points_len()`.
    pub fn point_util(&self, i: u8) -> u16 {
        let idx = i as usize;
        if idx >= self.0.model.len as usize {
            0
        } else {
            self.0.model.points[idx].util_bps
        }
    }

    /// Rate (bps) of the point at index `i`. Returns 0 if `i >= points_len()`.
    pub fn point_rate(&self, i: u8) -> u32 {
        let idx = i as usize;
        if idx >= self.0.model.len as usize {
            0
        } else {
            self.0.model.points[idx].rate_bps
        }
    }
}

#[wasm_bindgen]
impl FeedAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    /// Deserializes via Borsh through the shared `feed_state::Feed` type.
    pub fn from_bytes(account_data: &[u8]) -> Option<FeedAccount> {
        let mut data = account_data;
        Feed::try_deserialize(&mut data).ok().map(FeedAccount)
    }

    #[wasm_bindgen(getter)]
    pub fn collateral_price(&self) -> u64 {
        self.0.header.collateral_price
    }

    #[wasm_bindgen(getter)]
    pub fn lend_price(&self) -> u64 {
        self.0.header.lend_price
    }

    #[wasm_bindgen(getter)]
    pub fn last_updated_ts(&self) -> i64 {
        self.0.header.last_updated_ts
    }

    /// How long (milliseconds) this feed's price stays consumable. Owned by the
    /// feed, not by the markets pricing against it — see
    /// `interface::PriceFeedHeader::price_ttl_ms`.
    #[wasm_bindgen(getter)]
    pub fn price_ttl_ms(&self) -> u32 {
        self.0.header.price_ttl_ms
    }

    /// Delegates to `interface::PriceFeedHeader::is_stale_at`. Given the clock
    /// expected when the tx lands, returns `true` iff `borrow` /
    /// `withdraw_collateral` would reject the price as `StaleOracle`.
    ///
    /// Distinct from [`Self::is_pyth_price_stale`], which asks whether a *fresh
    /// Hermes update* could be posted — the written price may still be within
    /// its own TTL when that one says yes.
    pub fn is_price_stale(&self, clock_ts: i64) -> bool {
        self.0.header.is_stale_at(clock_ts)
    }

    /// Raw `PriceSource` discriminant:
    /// - `0` = Manual (authority sets prices directly)
    /// - `1` = Pyth (pull; `*_feed_id` are Pyth feed_id hashes; ephemeral `PriceUpdateV2`)
    /// - `2` = PythPush (sponsored push; `*_feed_id` are the sponsored `PriceUpdateV2` account pubkeys)
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> u8 {
        self.0.config.source as u8
    }

    /// True when the feed is bound to a sponsored Pyth push feed (`PriceSource::PythPush`).
    #[wasm_bindgen(getter)]
    pub fn is_push(&self) -> bool {
        self.0.config.source == feed_state::PriceSource::PythPush
    }

    #[wasm_bindgen(getter)]
    pub fn bump(&self) -> u8 {
        self.0.config.bump
    }

    #[wasm_bindgen(getter)]
    pub fn max_age_ms(&self) -> u32 {
        self.0.rules.max_age_ms
    }

    #[wasm_bindgen(getter)]
    pub fn collateral_decimals(&self) -> u8 {
        self.0.header.collateral_decimals
    }

    #[wasm_bindgen(getter)]
    pub fn lend_decimals(&self) -> u8 {
        self.0.header.lend_decimals
    }

    /// Authority pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        self.0.config.authority.to_bytes().to_vec()
    }

    /// Collateral mint pubkey as raw 32 bytes (the mint this feed was bound to at create time).
    #[wasm_bindgen(getter)]
    pub fn collateral_mint(&self) -> Vec<u8> {
        self.0.header.collateral_mint.to_bytes().to_vec()
    }

    /// Lend mint pubkey as raw 32 bytes (the mint this feed was bound to at create time).
    #[wasm_bindgen(getter)]
    pub fn lend_mint(&self) -> Vec<u8> {
        self.0.header.lend_mint.to_bytes().to_vec()
    }

    /// Collateral Pyth feed ID as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn collateral_feed_id(&self) -> Vec<u8> {
        self.0.config.collateral_feed_id.to_vec()
    }

    /// Lend Pyth feed ID as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn lend_feed_id(&self) -> Vec<u8> {
        self.0.config.lend_feed_id.to_vec()
    }

    /// Max Pyth confidence as fraction of price, in bps. `0` = disabled.
    #[wasm_bindgen(getter)]
    pub fn max_conf_bps(&self) -> u16 {
        self.0.rules.max_conf_bps
    }

    /// Time-scaled deviation budget in bps per hour. `0` = disabled.
    #[wasm_bindgen(getter)]
    pub fn max_deviation_bps_per_hour(&self) -> u16 {
        self.0.rules.max_deviation_bps_per_hour
    }

    /// Max allowed divergence between spot and Pyth EMA in bps. `0` = disabled.
    #[wasm_bindgen(getter)]
    pub fn ema_divergence_bps(&self) -> u16 {
        self.0.rules.ema_divergence_bps
    }

    /// Absolute floor on normalized price (PRICE_SCALE units). `0` = disabled.
    #[wasm_bindgen(getter)]
    pub fn min_price(&self) -> u64 {
        self.0.rules.min_price
    }

    /// Absolute ceiling on normalized price (PRICE_SCALE units). `0` = disabled.
    #[wasm_bindgen(getter)]
    pub fn max_price(&self) -> u64 {
        self.0.rules.max_price
    }

    /// Delegates to `feed_state::Feed::is_pyth_price_stale`. Callers pass their
    /// expected `set_from_pyth[_push]` landing time (usually `now +
    /// tx_confirmation_margin`) so the UI can predict `PriceTooOld` /
    /// `StalePushPrice` and disable the action before spending a transaction.
    pub fn is_pyth_price_stale(&self, publish_time: i64, clock_ts: i64) -> bool {
        self.0.is_pyth_price_stale(publish_time, clock_ts)
    }
}

// ── Core adapters ───────────────────────────────────────────────────────────
//
// Client-side equivalents of the program's `hooks::oracle::read_feed` and
// `hooks::irm::IrmState`. They expose the *real* on-chain inputs — the price
// read straight from the feed account, and the borrow rate evaluated from the
// IRM model at the pool's current utilization — so replaying a `Core` operation
// reproduces the on-chain result exactly. `current_ts` is sourced from
// `BrowserClock`, mirroring `Clock::get()` on-chain.

/// Delegates to the `interface::PriceFeedHeader` impl, which is the same code
/// the program runs — the ratio formula used to be transcribed here, which is
/// exactly the kind of second surface that can drift from consensus.
impl math::Oracle for FeedAccount {
    fn price(&self) -> u64 {
        math::Oracle::price(&self.0.header)
    }
}

/// Evaluates the real `IrmState` model at a fixed utilization to produce the
/// borrow rate `Core::accrue_interest` consumes.
struct IrmRateView {
    irm: IrmState,
    utilization: u64,
    current_ts: i64,
}
impl math::IrmRate for IrmRateView {
    fn rate_bps(&self) -> u32 {
        self.irm.model.get_fee_bps(self.utilization)
    }
    fn current_ts(&self) -> i64 {
        self.current_ts
    }
}

/// Wasm-exposed struct that combines a `Pool` account with its associated
/// `IrmState` and `Feed` accounts so that APY figures can be derived
/// client-side without an additional CPI or on-chain computation.
///
/// Construct via `PoolWithIrm::from_bytes(pool_bytes, irm_bytes, feed_bytes)`.
#[wasm_bindgen]
pub struct PoolWithIrm {
    pool: PoolAccount,
    irm: IrmConfigAccount,
    feed: FeedAccount,
}

impl PoolWithIrm {
    /// Effective utilization in basis points, matching the on-chain
    /// `Pool::calculate_utilization` used to query the borrow rate.
    fn utilization(&self) -> u64 {
        let m = &self.pool.0.market;
        math::utilization_bps(
            m.total_supply_assets,
            m.total_borrow_assets,
            m.assets_in_queue,
        )
    }

    /// Real IRM rate source for `Core`: the live model evaluated at the pool's
    /// current utilization, timestamped at `current_ts`.
    fn irm_rate(&self, current_ts: i64) -> IrmRateView {
        IrmRateView {
            irm: self.irm.0,
            utilization: self.utilization(),
            current_ts,
        }
    }
}

#[wasm_bindgen]
impl PoolWithIrm {
    /// Parse all three accounts from raw Anchor wire bytes (8-byte discriminator
    /// included in each slice). Returns `None` if any slice fails to parse.
    pub fn from_bytes(
        pool_bytes: &[u8],
        irm_bytes: &[u8],
        feed_bytes: &[u8],
    ) -> Option<PoolWithIrm> {
        let pool = PoolAccount::from_bytes(pool_bytes)?;
        let irm = IrmConfigAccount::from_bytes(irm_bytes)?;
        let feed = FeedAccount::from_bytes(feed_bytes)?;
        Some(PoolWithIrm { pool, irm, feed })
    }

    /// Returns the underlying `Feed` account wrapper.
    pub fn feed(&self) -> FeedAccount {
        self.feed
    }

    /// Returns the underlying `Pool` account wrapper.
    ///
    /// `Pool` is `bytemuck::Pod` (and therefore `Copy`), so this is a cheap
    /// value copy — no heap allocation.
    pub fn pool(&self) -> PoolAccount {
        PoolAccount(self.pool.0)
    }

    // ── Core replays ──────────────────────────────────────────────────────────
    //
    // Each method builds a `Core` from the real pool/position/feed/IRM state and
    // replays the matching on-chain operation, returning the value `Core` itself
    // computes. No share/amount formula is duplicated here.

    /// Interest that accrues on the pool's borrow balance up to now,
    /// using the live borrow rate from the IRM model. Replays
    /// `Core::accrue_interest` and returns the change in total borrowed.
    pub fn accrued_interest(&self) -> Option<u64> {
        let market = self.pool.0.market;
        let before = market.total_borrow_assets;
        let core = Core::new(market)
            .with_irm(self.irm_rate(crate::BrowserClock.current_ts()))
            .accrue_interest()?;
        core.market.total_borrow_assets.checked_sub(before)
    }

    /// Underlying lend tokens redeemable for `shares` LP tokens at the current
    /// pool ratio. Replays `Core::calc_lend_for_shares` — returns `None` when
    /// the pool has no LP supply yet.
    ///
    /// Accrues first, exactly as `withdraw_lent` does on-chain, so the quote
    /// matches what the instruction would actually pay out rather than a stale
    /// share price.
    pub fn lend_for_shares(&self, shares: u64) -> Option<u64> {
        Core::new(self.pool.0.market)
            .with_irm(self.irm_rate(crate::BrowserClock.current_ts()))
            .accrue_interest()?
            .calc_lend_for_shares(shares)
    }

    /// Oracle price from the feed account wired into this pool via `with_oracle`.
    /// Denominated in lend-raw units per collateral-raw unit, scaled by
    /// `PRICE_SCALE` (1_000_000). Divide by 1_000_000 to get the display-unit
    /// exchange rate when both tokens share the same decimal count.
    #[wasm_bindgen(getter)]
    pub fn oracle_price(&self) -> u64 {
        Core::new(self.pool.0.market)
            .with_oracle(self.feed)
            .oracle_price()
    }

    /// Projected LTV in bps after depositing `added_collateral_raw` more
    /// collateral tokens into `position`. Uses interest accrued to now.
    /// Returns `None` when debt is zero (no active borrow) or arithmetic overflows.
    pub fn projected_ltv_after_deposit(
        &self,
        position: &UserPositionAccount,
        added_collateral_raw: u64,
    ) -> Option<u32> {
        let debt = self.debt_amount(position)?;
        let new_collateral = position
            .0
            .collateral_deposited
            .checked_add(added_collateral_raw)?;
        math::compute_ltv(debt, new_collateral, self.oracle_price())
    }

    /// Projected health factor in bps after depositing `added_collateral_raw`
    /// more collateral tokens into `position`. Uses interest accrued to now.
    /// Returns `None` when debt is zero (no active borrow).
    pub fn projected_health_factor_after_deposit(
        &self,
        position: &UserPositionAccount,
        added_collateral_raw: u64,
    ) -> Option<u32> {
        let debt = self.debt_amount(position)?;
        let new_collateral = position
            .0
            .collateral_deposited
            .checked_add(added_collateral_raw)?;
        math::compute_health_factor(
            new_collateral,
            self.pool.0.market.ltv_percent,
            debt,
            self.oracle_price(),
        )
    }

    /// Debt shares minted if `position` borrows `amount` now, against a lend
    /// vault holding `vault_balance` raw units.
    ///
    /// Replays `Core::borrow`, so the result is `None` in exactly the cases the
    /// on-chain instruction rejects: over the LTV limit, or more than the vault
    /// can lend once assets reserved for the withdrawal queue are excluded.
    pub fn borrow_shares(
        &self,
        position: &UserPositionAccount,
        amount: u64,
        vault_balance: u64,
    ) -> Option<u64> {
        let mut core = Core::new(self.pool.0.market)
            .with_position(position.0)
            .with_oracle(self.feed)
            .with_irm(self.irm_rate(crate::BrowserClock.current_ts()))
            .accrue_interest()?;
        core.borrow(amount, vault_balance, |_| Ok::<(), ()>(())).ok()
    }

    /// Debt shares burned if `position` repays `amount` now.
    /// Replays `Core::repay`.
    pub fn repay_shares(
        &self,
        position: &UserPositionAccount,
        amount: u64,
    ) -> Option<u64> {
        let mut core = Core::new(self.pool.0.market)
            .with_position(position.0)
            .with_irm(self.irm_rate(crate::BrowserClock.current_ts()))
            .accrue_interest()?;
        core.repay(amount, |_| Ok::<(), ()>(()))
            .ok()
            .map(|(_, burned)| burned)
    }

    /// Current debt owed by `position`, after interest accrued to now.
    /// Replays a full `Core::repay` and returns the amount that clears it.
    pub fn debt_amount(&self, position: &UserPositionAccount) -> Option<u64> {
        let mut core = Core::new(self.pool.0.market)
            .with_position(position.0)
            .with_irm(self.irm_rate(crate::BrowserClock.current_ts()))
            .accrue_interest()?;
        core.repay(u64::MAX, |_| Ok::<(), ()>(()))
            .ok()
            .map(|(amount, _)| amount)
    }

    /// Maximum gross borrowable amount (raw lend units) for `position` at the
    /// pool's LTV and current oracle price — the exact ceiling the on-chain
    /// borrow check enforces via `Core::max_borrow_capacity`. Collateral value is
    /// converted to lend units through the feed price; callers subtract
    /// `debt_amount` for remaining headroom.
    pub fn max_borrowable(&self, position: &UserPositionAccount) -> Option<u64> {
        let core = Core::new(self.pool.0.market)
            .with_position(position.0)
            .with_oracle(self.feed);
        let oracle_price = core.oracle_price();
        core.max_borrow_capacity(position.0.collateral_deposited, oracle_price)
    }

    /// Loan-to-Value of `position` in basis points, using accrued debt.
    pub fn ltv(&self, position: &UserPositionAccount) -> Option<u32> {
        let debt = self.debt_amount(position)?;
        math::compute_ltv(debt, position.0.collateral_deposited, self.oracle_price())
    }

    /// Health Factor of `position` in basis points (1.0 = 10_000), using accrued debt.
    pub fn health_factor(&self, position: &UserPositionAccount) -> Option<u32> {
        let debt = self.debt_amount(position)?;
        math::compute_health_factor(
            position.0.collateral_deposited,
            self.pool.0.market.ltv_percent,
            debt,
            self.oracle_price(),
        )
    }

    /// Liquidation "price" (ratio) of `position` in basis points, using accrued debt.
    pub fn liq_price(&self, position: &UserPositionAccount) -> Option<u32> {
        let debt = self.debt_amount(position)?;
        math::compute_liquidation_threshold(
            debt,
            position.0.collateral_deposited,
            self.pool.0.market.ltv_percent,
        )
    }

    /// Human-readable debt of `position` as a decimal string, using accrued debt.
    pub fn format_debt(&self, position: &UserPositionAccount, decimals: u8) -> String {
        let amount = self.debt_amount(position).unwrap_or(0);
        format_token_capped(amount, decimals)
    }

    /// Net APY of a leveraged position at the given `leverage` multiple, in percent.
    /// `max(0, L × supplyAPY − (L−1) × borrowAPY)`.
    pub fn leveraged_net_apy(&self, leverage: f64) -> f64 {
        let supply_apy = self.supply_apy_bps() as f64 / 100.0;
        let borrow_apy = self.borrow_apy_bps() as f64 / 100.0;
        f64::max(0.0, leverage * supply_apy - (leverage - 1.0) * borrow_apy)
    }

    /// Borrow APY in basis points derived from the IRM fee curve at the
    /// pool's current utilization.
    ///
    /// Returns 0 when `total_supply_assets` is 0 (no deposits → no rate).
    pub fn borrow_apy_bps(&self) -> u32 {
        let util = self.pool.utilization_bps() as u64;
        self.irm.0.model.get_fee_bps(util)
    }

    /// Supply APY in basis points.
    ///
    /// Lenders earn the borrow rate weighted by utilization:
    ///   `supply_apy_bps = borrow_apy_bps × utilization_bps / 10_000`
    ///
    /// Returns 0 when `total_supply_assets` is 0.
    pub fn supply_apy_bps(&self) -> u32 {
        let util = self.pool.utilization_bps() as u64;
        let borrow_bps = self.irm.0.model.get_fee_bps(util) as u64;
        (borrow_bps.saturating_mul(util) / 10_000) as u32
    }

    /// Projected borrow APY in basis points after borrowing `additional_raw`
    /// lend-token base units on top of the current pool state.
    /// Returns 0 when total supply is 0.
    pub fn projected_borrow_apy_bps(&self, additional_raw: u64) -> u32 {
        let m = &self.pool.0.market;
        if m.total_supply_assets == 0 {
            return 0;
        }
        let new_borrow = m.total_borrow_assets.saturating_add(additional_raw);
        let util_bps = math::utilization_bps(m.total_supply_assets, new_borrow, m.assets_in_queue);
        self.irm.0.model.get_fee_bps(util_bps)
    }

    // ── PoolAccount delegates ─────────────────────────────────────────────────

    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        self.pool.authority()
    }

    #[wasm_bindgen(getter)]
    pub fn collateral_mint(&self) -> Vec<u8> {
        self.pool.collateral_mint()
    }

    #[wasm_bindgen(getter)]
    pub fn lend_mint(&self) -> Vec<u8> {
        self.pool.lend_mint()
    }

    #[wasm_bindgen(getter)]
    pub fn lp_mint(&self) -> Vec<u8> {
        self.pool.lp_mint()
    }

    #[wasm_bindgen(getter)]
    pub fn irm_state(&self) -> Vec<u8> {
        self.pool.irm_state()
    }

    #[wasm_bindgen(getter)]
    pub fn total_supply_assets(&self) -> u64 {
        self.pool.total_supply_assets()
    }

    #[wasm_bindgen(getter)]
    pub fn total_supply_shares(&self) -> u64 {
        self.pool.total_supply_shares()
    }

    #[wasm_bindgen(getter)]
    pub fn total_borrow_assets(&self) -> u64 {
        self.pool.total_borrow_assets()
    }

    #[wasm_bindgen(getter)]
    pub fn total_borrow_shares(&self) -> u64 {
        self.pool.total_borrow_shares()
    }

    #[wasm_bindgen(getter)]
    pub fn last_update(&self) -> i64 {
        self.pool.last_update()
    }

    #[wasm_bindgen(getter)]
    pub fn fee(&self) -> u64 {
        self.pool.fee()
    }

    #[wasm_bindgen(getter)]
    pub fn assets_in_queue(&self) -> u64 {
        self.pool.assets_in_queue()
    }

    #[wasm_bindgen(getter)]
    pub fn ltv_percent(&self) -> u8 {
        self.pool.ltv_percent()
    }

    #[wasm_bindgen(getter)]
    pub fn lp_mint_bump(&self) -> u8 {
        self.pool.lp_mint_bump()
    }

    /// The staleness budget that actually gates this market's borrows, taken
    /// from the feed it prices against rather than from the pool.
    #[wasm_bindgen(getter)]
    pub fn price_ttl_ms(&self) -> u32 {
        self.feed.price_ttl_ms()
    }

    pub fn pending_withdrawals(&self) -> u64 {
        self.pool.pending_withdrawals()
    }

    pub fn utilization_bps(&self) -> u16 {
        self.pool.utilization_bps()
    }

    pub fn available_liquidity(&self) -> u64 {
        self.pool.available_liquidity()
    }

    /// Max leverage a single flash-loop can reach for this pool.
    ///
    /// The borrow gate caps debt at `collateral × (oracle_price/PRICE_SCALE) × ltv`
    /// (`Core::max_borrow_capacity`). The loop ends at token ratio `(L−1)/L`
    /// collateral→debt, so the gate binds when `L ≤ 1/(1 − p·LTV)` where
    /// `p = oracle_price / PRICE_SCALE`. A 1% safety margin is applied and the
    /// result is floored to 0.1 steps so the slider ceiling never reverts on-chain.
    /// Returns 30.0 when the effective LTV ≥ 1 (degenerate ceiling).
    pub fn max_leverage(&self) -> f64 {
        let ltv_percent = self.pool.0.market.ltv_percent as f64;
        if ltv_percent <= 0.0 {
            return 1.0;
        }
        let oracle_price = self.oracle_price() as f64;
        if oracle_price <= 0.0 {
            // No usable price means no borrow capacity at all
            // (`max_borrow_capacity` is 0), so no loop can close. Falling back to
            // parity here offered a 4× slider on a pool that rejects every
            // borrow — the same "assume 1:1 when the oracle is silent" mistake
            // that made `compute_health_factor` overstate safety.
            return 1.0;
        }
        let price_factor = oracle_price / math::PRICE_SCALE as f64;
        let eff_ltv = (ltv_percent / 100.0) * price_factor;
        if eff_ltv >= 1.0 {
            return 30.0;
        }
        f64::max(1.0, (1.0 / (1.0 - eff_ltv) * 0.99 * 10.0).floor() / 10.0)
    }

    /// Net APY in basis points for a leveraged position at the given leverage.
    /// `leverage_bps`: leverage × 10_000 (e.g. 1.5× → 15_000, 30× → 300_000).
    /// Formula: `max(0, leverage × supply_apy_bps − (leverage−1) × borrow_apy_bps)`.
    pub fn leveraged_net_apy_bps(&self, leverage_bps: u32) -> u32 {
        let supply = self.supply_apy_bps() as i64;
        let borrow = self.borrow_apy_bps() as i64;
        let lev = leverage_bps as i64;
        let net = (lev * supply - (lev - 10_000) * borrow) / 10_000;
        net.max(0) as u32
    }
}

// ── feed freshness ────────────────────────────────────────────────────────────

/// Result of the combined feed freshness check — both the on-chain snapshot
/// gate and the Hermes-price gate. Produced by [`FeedFreshnessResult::check`].
#[wasm_bindgen]
pub struct FeedFreshnessResult {
    /// True when borrow/withdraw would revert `StaleOracle` and a Hermes refresh
    /// cannot fix it (source isn't pull-Pyth, or Hermes prices are themselves too old).
    pub will_fail: bool,
    /// True when the written price exceeds the feed's own `price_ttl_ms`.
    pub snapshot_stale: bool,
    /// Seconds since the on-chain snapshot was last written.
    pub snapshot_age_secs: i64,
    /// True when the collateral Hermes price is too old for `feed.rules.max_age_ms`.
    pub hermes_coll_stale: bool,
    /// True when the lend Hermes price is too old for `feed.rules.max_age_ms`.
    pub hermes_lend_stale: bool,
    /// Age (seconds) of the collateral Hermes price. 0 when unavailable.
    pub hermes_coll_age_secs: i64,
    /// Age (seconds) of the lend Hermes price. 0 when unavailable.
    pub hermes_lend_age_secs: i64,
    /// The feed's consumption budget in seconds, from `price_ttl_ms` — the gate
    /// `snapshot_stale` reports against (for display).
    pub price_ttl_secs: u32,
    /// The feed's Pyth *ingestion* budget in seconds, from `rules.max_age_ms`.
    /// A different gate with a different job; see `interface::price_stale_at`.
    pub feed_max_age_secs: u32,
}

#[wasm_bindgen]
impl FeedFreshnessResult {
    /// Evaluate the full feed freshness state.
    ///
    /// * `feed` – parsed feed account. The budget comes from the feed itself, so
    ///   the pool is no longer an input — every market pricing against a given
    ///   feed shares its freshness verdict.
    /// * `coll_hermes_ts` – Hermes `publish_time` for the collateral side (Unix seconds);
    ///   pass `0` when unavailable or when the feed is not pull-Pyth.
    /// * `lend_hermes_ts` – same for the lend side.
    /// * `now` – current Unix timestamp in seconds.
    ///
    /// When `snapshot_stale` is false the Hermes timestamps are ignored and both
    /// Hermes fields are returned as `false`/`0`. The caller is therefore free to
    /// skip the Hermes fetch entirely when the snapshot is fresh — call `check`
    /// once with `0, 0` to obtain the snapshot result, and only fetch Hermes (and
    /// call `check` again with real timestamps) when `snapshot_stale` is true.
    pub fn check(
        feed: &FeedAccount,
        coll_hermes_ts: i64,
        lend_hermes_ts: i64,
        now: i64,
    ) -> FeedFreshnessResult {
        let snapshot_age_secs = now.saturating_sub(feed.0.header.last_updated_ts);
        let snapshot_stale = feed.0.header.is_stale_at(now);
        let price_ttl_secs = feed.0.header.price_ttl_ms / 1000;
        let feed_max_age_secs = feed.0.rules.max_age_ms / 1000;

        if !snapshot_stale {
            return FeedFreshnessResult {
                will_fail: false,
                snapshot_stale: false,
                snapshot_age_secs,
                hermes_coll_stale: false,
                hermes_lend_stale: false,
                hermes_coll_age_secs: 0,
                hermes_lend_age_secs: 0,
                price_ttl_secs,
                feed_max_age_secs,
            };
        }

        // Stale snapshot, but the source can't be refreshed by the app.
        if feed.0.config.source != feed_state::PriceSource::Pyth {
            return FeedFreshnessResult {
                will_fail: true,
                snapshot_stale: true,
                snapshot_age_secs,
                hermes_coll_stale: false,
                hermes_lend_stale: false,
                hermes_coll_age_secs: 0,
                hermes_lend_age_secs: 0,
                price_ttl_secs,
                feed_max_age_secs,
            };
        }

        // Stale snapshot + pull-Pyth: check whether Hermes prices are fresh enough.
        let hermes_coll_stale =
            coll_hermes_ts == 0 || feed.0.is_pyth_price_stale(coll_hermes_ts, now);
        let hermes_lend_stale =
            lend_hermes_ts == 0 || feed.0.is_pyth_price_stale(lend_hermes_ts, now);
        let hermes_coll_age_secs = if coll_hermes_ts == 0 {
            0
        } else {
            now.saturating_sub(coll_hermes_ts)
        };
        let hermes_lend_age_secs = if lend_hermes_ts == 0 {
            0
        } else {
            now.saturating_sub(lend_hermes_ts)
        };

        FeedFreshnessResult {
            will_fail: hermes_coll_stale || hermes_lend_stale,
            snapshot_stale: true,
            snapshot_age_secs,
            hermes_coll_stale,
            hermes_lend_stale,
            hermes_coll_age_secs,
            hermes_lend_age_secs,
            price_ttl_secs,
            feed_max_age_secs,
        }
    }
}

// ── standalone display helpers ────────────────────────────────────────────────

/// Effective leverage of a position in basis points: `collateral / (collateral − debt) × 10_000`.
/// Returns 10_000 (= 1×) when equity ≤ 0 or collateral is 0.
#[wasm_bindgen]
pub fn position_leverage_bps(collateral: u64, debt: u64) -> u32 {
    if collateral == 0 || debt >= collateral {
        return 10_000;
    }
    let equity = collateral - debt;
    ((collateral as u128 * 10_000) / equity as u128).min(u32::MAX as u128) as u32
}

// ── formatting helpers ────────────────────────────────────────────────────────

/// Converts a raw token amount to a human-readable decimal string, capped at 6
/// decimal places for compact display. Trims trailing fractional zeros. This is
/// lossy past 6 places — use [`format_token_amount`] for values that must round
/// back to the same raw amount (e.g. filling an input).
///
/// Examples (decimals = 6):
///   1_000_000 → "1"
///   1_500_000 → "1.5"
///   1_234_567 → "1.234567"
///   1_234_560 → "1.23456"
fn format_token_capped(raw: u64, decimals: u8) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let scale = 10u64.pow(decimals as u32);
    let whole = raw / scale;
    let frac = raw % scale;
    if frac == 0 {
        return whole.to_string();
    }
    // Left-pad fractional part to `decimals` digits, then trim to ≤ 6 places.
    let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
    let max_places = 6_usize.min(decimals as usize);
    let trimmed = frac_str[..max_places].trim_end_matches('0');
    if trimmed.is_empty() {
        return whole.to_string();
    }
    format!("{}.{}", whole, trimmed)
}

// ── exact token-amount conversions (frontend I/O) ─────────────────────────────
//
// Base-10 scaling between a UI decimal string and raw integer minor units. These
// live here (not in the app) so the browser never re-expresses the on-chain
// integer/decimal convention in its own JS float math — the same reason protocol
// math replays `Core`. They are exact and round-trip: `parse_token_amount` floors
// exactly like the chain, `format_token_amount` reproduces the full value.

/// Parse a UI decimal string (e.g. `"12.345"`) into raw integer units for a mint
/// with `decimals` places. Excess fractional digits are truncated (floored),
/// never rounded up, so the result never exceeds what the user typed. Blank input
/// is `0`. Returns `None` on malformed input (non-digits, a sign, exponent) or on
/// `u64` overflow; callers treat that as "no amount". Plain decimal notation only.
#[wasm_bindgen]
pub fn parse_token_amount(value: &str, decimals: u8) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    let (int_part, frac_part) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let int_digits = if int_part.is_empty() { "0" } else { int_part };
    if !int_digits.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let dec = decimals as usize;
    let frac_trunc = &frac_part[..dec.min(frac_part.len())];
    let mut combined = String::with_capacity(int_digits.len() + dec);
    combined.push_str(int_digits);
    combined.push_str(frac_trunc);
    for _ in frac_trunc.len()..dec {
        combined.push('0');
    }
    combined.parse::<u64>().ok()
}

/// Format raw integer units into an exact UI decimal string with up to `decimals`
/// places, trimming only trailing fractional zeros. Inverse of
/// [`parse_token_amount`], so it is safe for filling input fields (e.g. a "Max"
/// button) where the string must map back to the same raw amount.
#[wasm_bindgen]
pub fn format_token_amount(raw: u64, decimals: u8) -> String {
    let dec = decimals as usize;
    if dec == 0 {
        return raw.to_string();
    }
    let padded = format!("{:0>width$}", raw, width = dec + 1);
    let split = padded.len() - dec;
    let int_part = &padded[..split];
    let frac_part = padded[split..].trim_end_matches('0');
    if frac_part.is_empty() {
        int_part.to_string()
    } else {
        format!("{int_part}.{frac_part}")
    }
}

/// Convert raw integer units to an `f64` for display, charts, and float math.
/// Routed through the exact decimal string so the result is the correctly-rounded
/// double in one step (unlike `raw / 10^decimals`, which rounds twice and drifts
/// past 2^53). Lossy — never use the result to reconstruct an on-chain amount.
#[wasm_bindgen]
pub fn token_amount_to_f64(raw: u64, decimals: u8) -> f64 {
    format_token_amount(raw, decimals).parse::<f64>().unwrap_or(0.0)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn account_bytes<T: Pod + bytemuck::Zeroable>() -> Vec<u8> {
        let mut v = vec![0u8; DISCRIMINATOR + core::mem::size_of::<T>()];
        v[..DISCRIMINATOR].fill(0xAA);
        v
    }

    /// Layout pin: both the program and the browser cast raw account bytes into
    /// these structs, so a size change that is not deliberate is silent
    /// corruption. Update these numbers only alongside an intended layout change.
    #[test]
    fn struct_sizes() {
        assert_eq!(core::mem::size_of::<Pool>(), 49_768);
        assert_eq!(core::mem::size_of::<UserPosition>(), 88);
    }

    #[test]
    fn rejects_short_slice() {
        let short = vec![0u8; DISCRIMINATOR + core::mem::size_of::<UserPosition>() - 1];
        assert!(UserPositionAccount::from_bytes(&short).is_none());
    }

    #[test]
    fn rejects_missing_discriminator() {
        let body_only = vec![0u8; core::mem::size_of::<UserPosition>()];
        assert!(UserPositionAccount::from_bytes(&body_only).is_none());
    }

    #[test]
    fn parses_zeroed_user_position() {
        let bytes = account_bytes::<UserPosition>();
        let pos = UserPositionAccount::from_bytes(&bytes)
            .expect("should parse")
            .0;
        assert_eq!(pos.collateral_deposited, 0);
        assert_eq!(pos.debt_shares, 0);
        assert_eq!(pos.bump, 0);
    }

    #[test]
    fn parses_user_position_known_values() {
        let mut bytes = account_bytes::<UserPosition>();
        bytes[DISCRIMINATOR] = 0xAB;
        bytes[DISCRIMINATOR + 31] = 0xCD;
        let cd_off = DISCRIMINATOR + 64;
        bytes[cd_off..cd_off + 8].copy_from_slice(&1_000_000u64.to_le_bytes());
        let ds_off = DISCRIMINATOR + 72;
        bytes[ds_off..ds_off + 8].copy_from_slice(&42u64.to_le_bytes());
        bytes[DISCRIMINATOR + 80] = 7;

        let pos = UserPositionAccount::from_bytes(&bytes)
            .expect("should parse")
            .0;
        assert_eq!(pos.authority.as_ref()[0], 0xAB);
        assert_eq!(pos.authority.as_ref()[31], 0xCD);
        assert_eq!(pos.collateral_deposited, 1_000_000);
        assert_eq!(pos.debt_shares, 42);
        assert_eq!(pos.bump, 7);
    }

    // ── PoolWithIrm helpers ───────────────────────────────────────────────────

    /// Build minimal valid wire bytes for a Feed account (all fields zeroed).
    /// `FeedAccount::from_bytes` deserializes via Anchor's `try_deserialize`,
    /// which validates the account discriminator — so it must be the real one,
    /// not a filler. A zeroed body is a valid `Manual` feed (source = 0).
    /// Wire bytes for a zero-priced feed.
    ///
    /// Serialized from a real `Feed` rather than a hand-counted byte length —
    /// the previous fixture hardcoded a body size and went silently out of date
    /// the moment the layout moved.
    fn feed_wire() -> Vec<u8> {
        use anchor_lang::{Discriminator, AnchorSerialize};
        let feed = Feed {
            header: feed_state::PriceFeedHeader {
                collateral_mint: anchor_lang::prelude::Pubkey::default(),
                lend_mint: anchor_lang::prelude::Pubkey::default(),
                collateral_price: 0,
                lend_price: 0,
                collateral_decimals: 0,
                lend_decimals: 0,
                last_updated_ts: 0,
                price_ttl_ms: 0,
            },
            id: 0,
            config: feed_state::FeedConfig {
                authority: anchor_lang::prelude::Pubkey::default(),
                source: feed_state::PriceSource::Manual,
                bump: 0,
                _pad: [0; 6],
                collateral_feed_id: [0; 32],
                lend_feed_id: [0; 32],
            },
            rules: feed_state::FeedRules::default(),
        };
        let mut v = Feed::DISCRIMINATOR.to_vec();
        feed.serialize(&mut v).unwrap();
        v
    }

    // Pool field offsets (from pool.rs layout comment):
    //   0..127   : 4 Pubkeys (authority, collateral_mint, lend_mint, lp_mint)
    //   128..199 : market (Market, 72 bytes)
    //     128..135 : total_supply_assets
    //     136..143 : total_supply_shares
    //     144..151 : total_borrow_assets
    //     152..159 : total_borrow_shares
    //     160..167 : last_update
    //     168..175 : fee
    //     176..183 : assets_in_queue
    //     184      : ltv_percent (u8)
    //     185..191 : _pad [u8; 7]
    const POOL_MARKET_OFFSET: usize = 128;

    // IrmState field offsets:
    //   0..31   : pool Pubkey
    //   32..71  : model (PiecewiseLinearModel = [RatePoint; 4] + len(u8) + _pad[7])
    //     RatePoint layout: rate_bps(u32) util_bps(u16) _pad[2]  (8 bytes)
    //     model.points starts at byte 32; model.len at byte 32 + 32 = 64
    //   72..103 : authority Pubkey
    //   104     : bump
    //   105..111: _pad[7]
    const IRM_POINTS_OFFSET: usize = 32; // relative to IrmState body (after discriminator)
    const IRM_LEN_OFFSET: usize = IRM_POINTS_OFFSET + 32; // 4 × 8-byte points

    /// Build wire bytes for a Pool with the given market supply and borrow amounts.
    /// All other fields are zeroed.
    fn pool_wire(total_supply_assets: u64, total_borrow_assets: u64) -> Vec<u8> {
        let mut v = account_bytes::<Pool>();
        let body = DISCRIMINATOR + POOL_MARKET_OFFSET;
        v[body..body + 8].copy_from_slice(&total_supply_assets.to_le_bytes());
        v[body + 16..body + 24].copy_from_slice(&total_borrow_assets.to_le_bytes());
        v
    }

    /// Build wire bytes for an IrmState with a flat rate `rate_bps` at every
    /// utilization — encoded as the 2-point curve `[(0, rate), (10_000, rate)]`.
    fn irm_wire_flat(rate_bps: u32) -> Vec<u8> {
        let mut v = account_bytes::<irm_state::IrmState>();
        // RatePoint layout: rate_bps(u32) util_bps(u16) _pad[2]  — 8 bytes.
        let base = DISCRIMINATOR + IRM_POINTS_OFFSET;

        // Point 0: (util=0, rate=rate_bps)
        v[base..base + 4].copy_from_slice(&rate_bps.to_le_bytes());
        v[base + 4..base + 6].copy_from_slice(&0u16.to_le_bytes());
        // Point 1: (util=10_000, rate=rate_bps)
        v[base + 8..base + 12].copy_from_slice(&rate_bps.to_le_bytes());
        v[base + 12..base + 14].copy_from_slice(&10_000u16.to_le_bytes());

        // len = 2
        v[DISCRIMINATOR + IRM_LEN_OFFSET] = 2;
        v
    }

    // ── PoolWithIrm tests ─────────────────────────────────────────────────────

    /// Zero supply → utilization is 0 bps.
    /// The borrow rate at zero utilization equals the curve's base rate `b`.
    /// The supply APY is 0 because supply_apy = borrow_bps × util_bps / 10_000
    /// and util_bps is 0.
    #[test]
    fn pool_with_irm_zero_supply_yields_zero_supply_apy() {
        let pool_bytes = pool_wire(0, 0);
        let irm_bytes = irm_wire_flat(500); // base rate 500 bps at any utilization
        let pwi =
            PoolWithIrm::from_bytes(&pool_bytes, &irm_bytes, &feed_wire()).expect("should parse");
        // Borrow rate at util=0 is the curve base rate (b=500).
        assert_eq!(pwi.borrow_apy_bps(), 500);
        // Supply APY is zero because utilization is zero (no deployed capital).
        assert_eq!(pwi.supply_apy_bps(), 0);
    }

    /// 50 % utilization (5 000 bps) with a flat rate of 800 bps:
    ///   borrow_apy = 800
    ///   supply_apy = 800 × 5_000 / 10_000 = 400
    #[test]
    fn pool_with_irm_flat_rate_half_utilization() {
        // 10_000 supplied, 5_000 borrowed → util = 5_000 bps
        let pool_bytes = pool_wire(10_000, 5_000);
        let irm_bytes = irm_wire_flat(800);
        let pwi =
            PoolWithIrm::from_bytes(&pool_bytes, &irm_bytes, &feed_wire()).expect("should parse");
        assert_eq!(pwi.borrow_apy_bps(), 800);
        assert_eq!(pwi.supply_apy_bps(), 400);
    }

    /// 100 % utilization (10 000 bps) with a flat rate of 1 200 bps:
    ///   borrow_apy = 1 200
    ///   supply_apy = 1 200 × 10_000 / 10_000 = 1 200
    #[test]
    fn pool_with_irm_full_utilization_supply_equals_borrow() {
        let pool_bytes = pool_wire(5_000, 5_000);
        let irm_bytes = irm_wire_flat(1_200);
        let pwi =
            PoolWithIrm::from_bytes(&pool_bytes, &irm_bytes, &feed_wire()).expect("should parse");
        assert_eq!(pwi.borrow_apy_bps(), 1_200);
        assert_eq!(pwi.supply_apy_bps(), 1_200);
    }

    /// Supply APY must always be ≤ borrow APY.
    #[test]
    fn pool_with_irm_supply_apy_never_exceeds_borrow_apy() {
        for util_pct in [0u64, 10, 25, 50, 75, 90, 100] {
            let supply = 10_000u64;
            let borrow = supply * util_pct / 100;
            let pool_bytes = pool_wire(supply, borrow);
            let irm_bytes = irm_wire_flat(1_000);
            let pwi = PoolWithIrm::from_bytes(&pool_bytes, &irm_bytes, &feed_wire())
                .expect("should parse");
            assert!(
                pwi.supply_apy_bps() <= pwi.borrow_apy_bps(),
                "supply_apy ({}) > borrow_apy ({}) at util {}%",
                pwi.supply_apy_bps(),
                pwi.borrow_apy_bps(),
                util_pct
            );
        }
    }

    /// `from_bytes` must return `None` when pool bytes are truncated.
    #[test]
    fn pool_with_irm_rejects_bad_pool_bytes() {
        let short_pool = vec![0u8; 4]; // way too short
        let irm_bytes = irm_wire_flat(500);
        assert!(PoolWithIrm::from_bytes(&short_pool, &irm_bytes, &feed_wire()).is_none());
    }

    /// `from_bytes` must return `None` when IRM bytes are truncated.
    #[test]
    fn pool_with_irm_rejects_bad_irm_bytes() {
        let pool_bytes = pool_wire(10_000, 5_000);
        let short_irm = vec![0u8; 4];
        assert!(PoolWithIrm::from_bytes(&pool_bytes, &short_irm, &feed_wire()).is_none());
    }

    /// `pool()` returns a `PoolAccount` with the same underlying data.
    #[test]
    fn pool_with_irm_pool_getter_roundtrips_data() {
        let pool_bytes = pool_wire(12_345, 6_789);
        let irm_bytes = irm_wire_flat(300);
        let pwi =
            PoolWithIrm::from_bytes(&pool_bytes, &irm_bytes, &feed_wire()).expect("should parse");
        let returned_pool = pwi.pool();
        assert_eq!(returned_pool.total_supply_assets(), 12_345);
        assert_eq!(returned_pool.total_borrow_assets(), 6_789);
    }
}

#[cfg(test)]
mod token_amount_tests {
    use super::{format_token_amount, parse_token_amount, token_amount_to_f64};

    #[test]
    fn parse_basic_and_truncates() {
        assert_eq!(parse_token_amount("1", 6), Some(1_000_000));
        assert_eq!(parse_token_amount("1.5", 6), Some(1_500_000));
        assert_eq!(parse_token_amount(".5", 6), Some(500_000));
        assert_eq!(parse_token_amount("0.008344", 6), Some(8_344));
        // Excess precision is floored, never rounded up.
        assert_eq!(parse_token_amount("12.345678999", 6), Some(12_345_678));
        assert_eq!(parse_token_amount("1.9999999", 6), Some(1_999_999));
    }

    #[test]
    fn parse_edge_cases() {
        assert_eq!(parse_token_amount("", 6), Some(0));
        assert_eq!(parse_token_amount("   ", 6), Some(0));
        assert_eq!(parse_token_amount("42", 0), Some(42));
        // Malformed input is rejected, not silently coerced.
        assert_eq!(parse_token_amount("-1", 6), None);
        assert_eq!(parse_token_amount("1e3", 6), None);
        assert_eq!(parse_token_amount("1.2.3", 6), None);
        assert_eq!(parse_token_amount("abc", 6), None);
        // Overflow past u64 returns None.
        assert_eq!(parse_token_amount("18446744073710", 6), None);
    }

    #[test]
    fn format_trims_zeros() {
        assert_eq!(format_token_amount(1_000_000, 6), "1");
        assert_eq!(format_token_amount(1_500_000, 6), "1.5");
        assert_eq!(format_token_amount(8_344, 6), "0.008344");
        assert_eq!(format_token_amount(0, 6), "0");
        assert_eq!(format_token_amount(42, 0), "42");
        // Full precision retained past 6 places (unlike the capped display fmt).
        assert_eq!(format_token_amount(1_234_567_890, 9), "1.23456789");
    }

    #[test]
    fn parse_format_roundtrip() {
        for raw in [0u64, 1, 8_344, 1_000_000, 12_345_678, u64::MAX] {
            let s = format_token_amount(raw, 9);
            assert_eq!(parse_token_amount(&s, 9), Some(raw), "roundtrip {raw}");
        }
    }

    #[test]
    fn to_f64_exact_for_small_values() {
        assert_eq!(token_amount_to_f64(1_500_000, 6), 1.5);
        assert_eq!(token_amount_to_f64(8_344, 6), 0.008344);
        assert_eq!(token_amount_to_f64(0, 6), 0.0);
    }
}

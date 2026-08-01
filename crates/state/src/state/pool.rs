use crate::withdrawal_queue::WithdrawalQueue;
use anchor_lang::prelude::*;

/// Per-market accounting: supply, borrow, and fee state.
#[zero_copy]
pub struct Market {
    /// Sum of lend tokens currently deposited (basis for LP ratio and utilisation).
    pub total_supply_assets: u64,
    /// Total LP tokens outstanding for the lend side.
    pub total_supply_shares: u64,
    pub total_borrow_assets: u64,
    pub total_borrow_shares: u64,
    pub last_update: i64,
    /// Protocol fee **rate** in basis points, skimmed from accrued interest.
    ///
    /// Fixed at 0 for every pool: `create` stamps it and no instruction changes
    /// it (the `set_fee` entrypoint was removed). The field and the accrual math
    /// that reads it stay in place so a fee can be reintroduced without a layout
    /// change, but as shipped no interest is ever skimmed.
    pub fee: u64,
    pub assets_in_queue: u64,
    pub ltv_percent: u8,
    _pad: [u8; 7],
    /// Protocol-owned supply shares accrued from the fee, not yet claimed as LP.
    /// Minted into `total_supply_shares` at accrual; `claim_fees` mints matching
    /// LP tokens to the pool authority and resets this to 0.
    pub accrued_fee_shares: u64,
    /// Principal of the flash loan currently in flight, non-zero only *between*
    /// the `flash_borrow` and `flash_repay` instructions of a single
    /// transaction.
    ///
    /// This is the pairing lock: `Core::flash_borrow` refuses to run unless it is
    /// 0 and then sets it, so a second borrow cannot open while one is
    /// outstanding, and `Core::flash_repay` must clear exactly it. Scanning the
    /// instruction sysvar alone could not enforce that — several borrows were
    /// able to point at one repay.
    pub flash_loan_outstanding: u64,
}

impl math::Market for Market {
    fn total_supply_assets(&self) -> u64 {
        self.total_supply_assets
    }
    fn total_supply_shares(&self) -> u64 {
        self.total_supply_shares
    }
    fn total_borrow_assets(&self) -> u64 {
        self.total_borrow_assets
    }
    fn total_borrow_shares(&self) -> u64 {
        self.total_borrow_shares
    }
    fn last_update(&self) -> i64 {
        self.last_update
    }
    fn fee(&self) -> u64 {
        self.fee
    }
    fn assets_in_queue(&self) -> u64 {
        self.assets_in_queue
    }
    fn ltv_percent(&self) -> u8 {
        self.ltv_percent
    }
    fn flash_loan_outstanding(&self) -> u64 {
        self.flash_loan_outstanding
    }
    fn total_supply_assets_mut(&mut self) -> &mut u64 {
        &mut self.total_supply_assets
    }
    fn total_supply_shares_mut(&mut self) -> &mut u64 {
        &mut self.total_supply_shares
    }
    fn accrued_fee_shares(&self) -> u64 {
        self.accrued_fee_shares
    }
    fn accrued_fee_shares_mut(&mut self) -> &mut u64 {
        &mut self.accrued_fee_shares
    }
    fn assets_in_queue_mut(&mut self) -> &mut u64 {
        &mut self.assets_in_queue
    }
    fn total_borrow_assets_mut(&mut self) -> &mut u64 {
        &mut self.total_borrow_assets
    }
    fn total_borrow_shares_mut(&mut self) -> &mut u64 {
        &mut self.total_borrow_shares
    }
    fn last_update_mut(&mut self) -> &mut i64 {
        &mut self.last_update
    }
    fn flash_loan_outstanding_mut(&mut self) -> &mut u64 {
        &mut self.flash_loan_outstanding
    }
}

/// Unified lending pool account stored as zero-copy.
///
/// Manages three token mints:
///   - `collateral_mint`: tokens users deposit as collateral to enable borrowing
///   - `lend_mint`:       tokens lenders deposit (earning LP) and borrowers receive
///   - `lp_mint`:         issued 1:1 to lend-side depositors; redeemable for lend tokens
///
#[account(zero_copy)]
pub struct Pool {
    pub authority: Pubkey,
    /// Mint of the token deposited as collateral (tracked raw, no LP issued).
    pub collateral_mint: Pubkey,
    /// Mint of the token lenders deposit and borrowers receive.
    pub lend_mint: Pubkey,
    /// LP token mint issued to lend-side depositors.
    pub lp_mint: Pubkey,
    pub market: Market,
    /// IRM program ID. All borrow-rate queries are made via CPI to this program.
    pub rate_program: Pubkey,
    /// IRM state account (PDA) passed to the rate program CPI.
    pub irm_state: Pubkey,
    /// Feed program ID used for price oracle queries.
    pub feed_program: Pubkey,
    /// Feed state account (PDA) passed to the feed program.
    pub feed_state: Pubkey,
    pub lp_mint_bump: u8,
    _pad: [u8; 7], // explicit padding — no implicit/uninitialised bytes
    /// Queue of pending lend-token withdrawals (LP burned at `leave` time).
    pub withdrawal_queue: WithdrawalQueue,
    /// Maximum age (**milliseconds**) the oracle's `last_updated_ts` may be
    /// behind the current clock before borrow / withdraw_collateral reject the
    /// price as stale. Set at pool creation; chosen per market based on the
    /// underlying feed's update cadence.
    ///
    /// Milliseconds to match `FeedRules::max_age_ms`, so a market's staleness
    /// budget and its feed's are expressed in the same unit and neither is
    /// truncated. The underlying timestamps are Unix seconds, so the measured
    /// age still moves in whole seconds — the unit buys threshold precision, not
    /// clock precision. `u32` ms spans ~49 days.
    pub max_feed_age_ms: u32,
    _pad1: [u8; 4], // align the fields below (u64/Pubkey runs want 8-byte alignment)
    /// Whitelist this market is gated on, or [`Pubkey::default`] for an open
    /// market. Bound at pool creation and never mutable afterwards.
    ///
    /// Pinning it here is what makes a per-authority guard safe. The whitelist
    /// PDA is `["guard", guard_authority]` in the `guard` program, so anyone can
    /// stand one up naming themselves — a consumer that only checked the guard
    /// *program* would be handed a whitelist the caller had just added
    /// themselves to. Recording the exact state account at creation means every
    /// later gate compares against a fixed address the market committed to up
    /// front, and lenders can inspect `guard_state` before depositing to see
    /// whose list they are trusting.
    ///
    /// Consumed from `_reserved` rather than appended, so `Pool`'s total size is
    /// unchanged and no live account needs migrating.
    pub guard_state: Pubkey,
    /// Growth room, in 8-byte slots, for fields added after launch.
    ///
    /// Sized deliberately large: liquidation is still to come and will need
    /// several fields (close factor, incentive/bonus bps, a liquidation LTV
    /// distinct from the borrow LTV, possibly a paused flag and bad-debt
    /// counter). Adding a field consumes a slot here and leaves `Pool`'s total
    /// size — and therefore every live account's rent and layout — untouched,
    /// so no migration is needed. Growing this later is the migration; growing
    /// it now is free.
    ///
    /// `Pool` is already ~49 KB (dominated by the 1024-entry withdrawal queue),
    /// so the remaining slots are well under 1% of the account.
    ///
    /// 32 slots at launch; `guard_state` took 4, leaving 28.
    _reserved: [u64; 28],
}

impl Pool {
    pub fn calculate_utilization(&self) -> u64 {
        math::utilization_bps(
            self.market.total_supply_assets,
            self.market.total_borrow_assets,
            self.market.assets_in_queue,
        )
    }

    /// `true` iff a feed snapshot with `feed_last_updated_ts` would trip
    /// this pool's `StaleOracle` gate when read at wall-clock `clock_ts`.
    /// Single source of truth for the borrow / withdraw freshness
    /// check in `programs/calma/src/hooks/oracle.rs`; the wasm bindings expose
    /// this so the UI can predict `StaleOracle` and skip / prompt a refresh
    /// before submitting the tx.
    pub fn is_feed_snapshot_stale(&self, feed_last_updated_ts: i64, clock_ts: i64) -> bool {
        Self::snapshot_stale_at(feed_last_updated_ts, clock_ts, self.max_feed_age_ms)
    }

    /// Same predicate as [`Self::is_feed_snapshot_stale`] but with `max_age_ms`
    /// passed explicitly — the `create` instruction runs the check *before*
    /// a `Pool` exists to bind the config to.
    ///
    /// Compared in milliseconds so the configured budget is applied exactly as
    /// written, with no seconds truncation. `i128` keeps the ×1000 total even at
    /// extreme timestamps.
    ///
    /// # `max_age_ms == 0` means "reject", not "disabled"
    ///
    /// This is the **opposite** of `feed::rules::check_max_age`, where `0` is the
    /// disabled sentinel shared by every `FeedRules` gate so a
    /// zero-init rules struct reproduces the pre-rules behavior. The two
    /// predicates read almost identically and take an identically named
    /// `max_age_ms`, so the divergence is stated here rather than left to be
    /// inferred: on the feed-ingestion side `0` opts out of a validation, while
    /// here it is a pool config that has to gate every borrow, and the only safe
    /// reading of a missing budget on a *gate* is to fail closed.
    ///
    /// `create` already requires `max_feed_age_ms > 0`, so a live pool cannot
    /// carry `0` — this branch exists so that if that guard is ever relaxed, or
    /// a caller passes `0` directly, the result is a refused borrow rather than
    /// an oracle check that silently stopped running. Without it, `0` behaved as
    /// a one-second budget by accident (only a same-second price passed), which
    /// is neither convention and reads as intentional to nobody.
    pub fn snapshot_stale_at(feed_last_updated_ts: i64, clock_ts: i64, max_age_ms: u32) -> bool {
        if max_age_ms == 0 {
            return true;
        }
        let elapsed_ms = (clock_ts as i128 - feed_last_updated_ts as i128) * 1_000;
        elapsed_ms > max_age_ms as i128
    }
}

#[cfg(test)]
mod tests {
    use super::Pool;

    const NOW: i64 = 1_700_000_000;

    #[test]
    fn zero_budget_fails_closed() {
        // Every case is stale, including the same-second price that used to slip
        // through and the future-dated one that passes under any real budget.
        assert!(Pool::snapshot_stale_at(NOW, NOW, 0));
        assert!(Pool::snapshot_stale_at(NOW - 1, NOW, 0));
        assert!(Pool::snapshot_stale_at(NOW + 5, NOW, 0));
    }

    #[test]
    fn budget_is_applied_in_milliseconds() {
        // 90s elapsed against a 90_000 ms budget: exactly at the limit, fresh.
        assert!(!Pool::snapshot_stale_at(NOW - 90, NOW, 90_000));
        // One second past it.
        assert!(Pool::snapshot_stale_at(NOW - 91, NOW, 90_000));
        // Sub-second budgets are not truncated to zero: 1_500 ms admits a
        // one-second-old price and refuses a two-second-old one.
        assert!(!Pool::snapshot_stale_at(NOW - 1, NOW, 1_500));
        assert!(Pool::snapshot_stale_at(NOW - 2, NOW, 1_500));
    }

    #[test]
    fn a_price_published_ahead_of_the_clock_is_fresh() {
        // Publishers can run slightly early; that is not staleness.
        assert!(!Pool::snapshot_stale_at(NOW + 5, NOW, 1_000));
    }

    #[test]
    fn extreme_timestamps_stay_total() {
        // The ×1000 is done in i128 so neither bound can overflow or wrap.
        assert!(Pool::snapshot_stale_at(i64::MIN, i64::MAX, u32::MAX));
        assert!(!Pool::snapshot_stale_at(i64::MAX, i64::MIN, u32::MAX));
    }
}

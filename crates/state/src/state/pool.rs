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
    /// Program that owns this market's price account.
    ///
    /// Any program can serve as an oracle, so this is not a compile-time
    /// constant — it is the market's recorded choice, and `interface`'s reader
    /// checks the price account's owner against it on every read. Together with
    /// `feed_state` it is the whole oracle trust model, so it is the field a
    /// depositor should inspect before entering the market.
    pub feed_program: Pubkey,
    /// The exact price account this market prices against. Must be owned by
    /// `feed_program` and lead with an `interface::PriceFeedHeader`.
    pub feed_state: Pubkey,
    pub lp_mint_bump: u8,
    _pad: [u8; 7], // explicit padding — no implicit/uninitialised bytes
    /// Queue of pending lend-token withdrawals (LP burned at `leave` time).
    pub withdrawal_queue: WithdrawalQueue,
    /// Was `max_feed_age_ms`, the market's own staleness budget for the oracle.
    /// The budget is now the oracle's to set — see
    /// `interface::PriceFeedHeader::price_ttl_ms` — because the feed knows its
    /// own update cadence and a market cannot sensibly guess it. Left as padding
    /// rather than removed so `Pool`'s layout, and every live account's rent,
    /// stay untouched.
    _pad1: [u8; 8],
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
    /// Program asked to vouch for `guard_state`, or [`Pubkey::default`] for an
    /// open market. Bound at creation alongside it and never mutable after.
    ///
    /// Pinning the *program* as well as the state is what makes the gate real.
    /// The state address alone would let a caller pass their own program next to
    /// the correct whitelist account and have it answer `Ok` without reading it —
    /// `calma` hands the account over and believes the reply, so it cannot tell
    /// an implementation from an impostor.
    ///
    /// Recorded per market rather than compared against one canonical program
    /// id, matching `feed_program` and `rate_program`: no external program is
    /// privileged at compile time, and every market states which ones it trusts.
    pub guard_program: Pubkey,
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
    /// 32 slots at launch; `guard_state` and `guard_program` took 4 each,
    /// leaving 24.
    _reserved: [u64; 24],
}

impl Pool {
    pub fn calculate_utilization(&self) -> u64 {
        math::utilization_bps(
            self.market.total_supply_assets,
            self.market.total_borrow_assets,
            self.market.assets_in_queue,
        )
    }

    /// The whitelist this market is gated on, or a pair of `Pubkey::default()`
    /// for an open market. See `guard_state`/`guard_program` for the trust
    /// model.
    pub fn guard_config(&self) -> (Pubkey, Pubkey) {
        (self.guard_state, self.guard_program)
    }

    /// The oracle this market prices against. See `feed_state`/`feed_program`
    /// for the trust model.
    pub fn feed_config(&self) -> (Pubkey, Pubkey) {
        (self.feed_state, self.feed_program)
    }
}

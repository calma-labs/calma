//! A market's price *and* its rate model, from one program and one account.
//!
//! `calma` depends on two external things per market: an oracle it reads a price
//! from, and a rate model it asks a borrow rate of. Both are interfaces rather
//! than fixed programs — a market records which program serves each role at
//! creation and is pinned to that choice for life. The reference implementations
//! are `programs/feed` (prices) and `programs/irm` (rates); this is a second,
//! independent one that fills both roles at once.
//!
//! # What it takes to be a price provider
//!
//! Own an account that leads with an [`interface::PriceFeedHeader`], directly
//! after the 8-byte discriminator. That is the entire contract. `calma` reads
//! the header straight out of the account — no CPI — and trusts it because the
//! account's owner matches the program the market recorded. The discriminator is
//! skipped without being checked (with pluggable providers there is no single
//! value to check against), and everything below the header is ignored, which is
//! what lets [`state::Provider`] carry rate-model fields underneath.
//!
//! # What it takes to be a rate provider
//!
//! Three things, none of which are visible from `calma`'s source:
//!
//! 1. **Instruction names.** `calma` dispatches to `pool.rate_program` at
//!    runtime but encodes the call with the reference program's generated
//!    helpers, so what travels is Anchor's discriminator —
//!    `sha256("global:borrow_rate")[..8]` and `sha256("global:check_authority")[..8]`.
//!    The names must match character for character. Argument types
//!    (`utilization_bps: u64`, `authority: Pubkey`) and the `u32` return-data
//!    encoding follow from that.
//! 2. **Account order.** Exactly two accounts, rate account then pool, neither
//!    signer nor writable. Field *names* are free; position is the contract.
//! 3. **PDA seeds.** `["irm_config", pool]` under your own program id.
//!    `calma::create` re-derives it and refuses anything else.
//!
//! Nothing constrains the *model*. The reference `irm` interpolates a piecewise
//! curve; this program returns a constant at every utilization. `calma` consumes
//! a `u32` and never learns which.
//!
//! # Serving both roles from one account
//!
//! [`state::Provider`] sits at `["irm_config", pool]` — the address the rate role
//! requires — and leads with the price header the oracle role requires. A market
//! is created naming the same pubkey for `feed_state` and `irm_state`, and the
//! account is then exercised through a direct read and a CPI in the same
//! instruction.

pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use error::ErrorCode;
pub use instructions::*;
pub use state::*;

declare_id!("QUoTDMDSEw1nAFRp27eWrK4AcVsE1Sg9YupYcc22Yhc");

/// Ceiling on the rate this provider will quote: 50% APY, matching the
/// reference `irm`'s `MAX_RATE_BPS`. A flat model needs the cap even more than a
/// curve does — there is no interpolation to soften a mistyped value.
pub const MAX_RATE_BPS: u32 = 5_000;

#[program]
pub mod quote {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        price_ttl_ms: u32,
        flat_rate_bps: u32,
        collateral_price: u64,
        lend_price: u64,
    ) -> Result<()> {
        initialize::initialize_handler(
            ctx,
            price_ttl_ms,
            flat_rate_bps,
            collateral_price,
            lend_price,
        )
    }

    pub fn set_price(ctx: Context<SetValue>, collateral_price: u64, lend_price: u64) -> Result<()> {
        set_value::set_price_handler(ctx, collateral_price, lend_price)
    }

    pub fn set_rate(ctx: Context<SetValue>, flat_rate_bps: u32) -> Result<()> {
        set_value::set_rate_handler(ctx, flat_rate_bps)
    }

    pub fn set_price_ttl(ctx: Context<SetValue>, price_ttl_ms: u32) -> Result<()> {
        set_value::set_price_ttl_handler(ctx, price_ttl_ms)
    }

    // ── The rate-provider ABI. Names are load-bearing; see the module docs. ──

    pub fn borrow_rate(ctx: Context<RateQuery>, utilization_bps: u64) -> Result<u32> {
        borrow_rate::borrow_rate_handler(ctx, utilization_bps)
    }

    pub fn check_authority(ctx: Context<RateQuery>, authority: Pubkey) -> Result<()> {
        borrow_rate::check_authority_handler(ctx, authority)
    }
}

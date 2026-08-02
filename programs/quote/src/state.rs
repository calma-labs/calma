use anchor_lang::prelude::*;
use interface::PriceFeedHeader;

/// One account serving both of a market's external dependencies.
///
/// `calma` keeps its oracle and its rate model apart — the price is *read* off
/// an account, the rate is *asked for* over CPI — but nothing says the two have
/// to come from different places. This account sits at the address `calma`
/// requires of a rate account and carries, at its front, the layout `calma`
/// requires of a price account, so a market can name it for both roles.
///
/// # The header must stay first
///
/// `interface::read_price_feed` skips the 8-byte discriminator and deserializes
/// a [`PriceFeedHeader`] from what follows. Everything below the header is this
/// program's own business and is never looked at by a consumer — which is what
/// lets this account carry rate-model fields that no price account would have.
#[account]
pub struct Provider {
    pub header: PriceFeedHeader,
    /// The market this provider serves. Checked with `has_one` on every
    /// instruction that takes a pool, so a provider account cannot be pointed at
    /// a market it was not created for.
    pub pool: Pubkey,
    /// The key allowed to move the price and the rate. Reported to `calma` over
    /// the `check_authority` ABI method at market creation.
    pub authority: Pubkey,
    pub bump: u8,
    /// The borrow rate, in basis points, at **every** utilization.
    ///
    /// Deliberately not a curve. `calma` asks for a rate over CPI and never sees
    /// the model that produced it, so a provider is free to have no model at
    /// all — the reference `irm` program's piecewise-linear curve is one choice,
    /// not the contract.
    pub flat_rate_bps: u32,
}

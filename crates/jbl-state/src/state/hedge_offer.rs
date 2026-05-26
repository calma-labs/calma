use anchor_lang::prelude::*;

/// An offer to hedge interest-rate risk at a fixed rate.
///
/// The creator locks `collateral_deposited` tokens as collateral for the duration of any
/// accepted match. The PDA seeds encode the offer parameters so two distinct offers with
/// different rates or durations cannot alias each other.
///
/// Seeds: `["rate_hedge_offer", pool, authority, fixed_rate_le, min_duration_le, max_duration_le]`
///
/// Memory layout (repr C, no implicit padding):
///   offsets 0-31  : pool        (32 bytes)
///   offsets 32-63 : authority   (32 bytes)
///   offsets 64-71 : amount      (8)
///   offsets 72-79 : fixed_rate_bps (8)
///   offsets 80-87 : min_duration (8)
///   offsets 88-95 : max_duration (8)
///   offsets 96-103: collateral_deposited (8)
///   offsets 104-111: locked_tokens (8)
///   offset 112    : bump        (1)
///   offsets 113-119: _pad       (7)
#[account(zero_copy)]
pub struct RateHedgeOffer {
    /// The pool this offer is associated with.
    pub pool: Pubkey,
    /// The user who created the offer.
    pub authority: Pubkey,
    /// Notional amount of lend tokens covered by this offer.
    pub amount: u64,
    /// Fixed interest rate in basis points (e.g. 500 = 5.00 %).
    pub fixed_rate_bps: u64,
    /// Minimum acceptable duration for a match, in seconds.
    pub min_duration: u64,
    /// Maximum acceptable duration for a match, in seconds.
    pub max_duration: u64,
    /// Collateral tokens locked as security when the offer was created.
    pub collateral_deposited: u64,
    /// Running total of upfront fees owed to the offer creator across all active matches.
    /// At match settlement these tokens are transferred from the lend vault to the creator.
    pub locked_tokens: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

/// Records an active rate-hedge match between a borrower and an offer creator.
///
/// Created atomically with a hedged borrow. Lives until the crank settles it after
/// `start_ts + duration` has elapsed.
///
/// Seeds: `["rate_hedge_match", user_position]`
/// (one active match per user position at a time)
///
/// Memory layout (repr C, no implicit padding):
///   offsets 0-31  : offer       (32 bytes)
///   offsets 32-63 : user_position (32 bytes)
///   offsets 64-71 : amount      (8)
///   offsets 72-79 : upfront_fee (8)
///   offsets 80-87 : initial_debt_shares (8)
///   offsets 88-95 : start_ts    (8)
///   offsets 96-103: duration    (8)
///   offset 104    : bump        (1)
///   offsets 105-111: _pad       (7)
#[account(zero_copy)]
pub struct RateHedgeMatch {
    /// The offer backing this match.
    pub offer: Pubkey,
    /// The borrower's position account.
    pub user_position: Pubkey,
    /// Original borrow amount (lend tokens received by the borrower).
    /// This is the borrower's repayment cap — they will never owe more than this
    /// as a result of the hedge.
    pub amount: u64,
    /// Upfront fixed fee paid into the pool as additional debt shares.
    /// At settlement this is transferred from the lend vault to the offer creator.
    pub upfront_fee: u64,
    /// Debt shares issued at match creation, covering `amount + upfront_fee`.
    /// Used at settlement to compute how much the variable rate actually grew.
    pub initial_debt_shares: u64,
    /// Unix timestamp when the match was created.
    pub start_ts: i64,
    /// Agreed hedge duration in seconds.
    pub duration: u64,
    pub bump: u8,
    _pad: [u8; 7],
}

use anchor_lang::prelude::*;

use crate::state::Provider;

/// Accounts for the two ABI methods `calma` calls on a rate provider.
///
/// **Position, not name, is the contract.** `calma` builds its CPI from the
/// reference implementation's generated helpers
/// (`irm::cpi::accounts::BorrowRate`), which emit exactly two account metas —
/// the rate account then the pool, neither signer nor writable. Any provider
/// whose first two accounts have that shape is call-compatible; what the fields
/// are called here only affects this program's own IDL.
#[derive(Accounts)]
pub struct RateQuery<'info> {
    /// Seed-derived so the account is provably the canonical
    /// `["irm_config", pool]` PDA for this market, and `has_one` so it cannot be
    /// some other market's provider carrying a forged `pool` field.
    ///
    /// `calma` already checks this address at market creation. Checking it again
    /// here is not redundant: the two checks answer to different parties, and a
    /// rate model that trusted its caller to have picked the right account would
    /// quote one market's rate for another's debt.
    #[account(
        seeds = [b"irm_config", pool.key().as_ref()],
        bump = provider.bump,
        has_one = pool,
    )]
    pub provider: Account<'info, Provider>,

    /// CHECK: verified via the has_one constraint on `provider`.
    pub pool: UncheckedAccount<'info>,
}

/// The borrow rate at `utilization_bps` — which this provider ignores.
///
/// A flat rate is the whole point: `calma` consumes the returned `u32` and never
/// learns how it was produced, so "the rate model" is not a shape the protocol
/// imposes. The reference `irm` interpolates a 2–4 point curve; this returns a
/// constant; a third provider could read a governance feed. All three are the
/// same interface.
pub fn borrow_rate_handler(ctx: Context<RateQuery>, utilization_bps: u64) -> Result<u32> {
    let rate = ctx.accounts.provider.flat_rate_bps;
    msg!(
        "quote::borrow_rate utilization={} rate={} (flat)",
        utilization_bps,
        rate
    );
    Ok(rate)
}

/// Assert that `authority` controls this provider.
///
/// Asked over CPI rather than read off the account because `calma` does not know
/// this program's layout — that is precisely what makes the layout ours to
/// choose. See `initialize` for why the check exists at all.
pub fn check_authority_handler(ctx: Context<RateQuery>, authority: Pubkey) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.provider.authority,
        authority,
        crate::error::ErrorCode::Unauthorized
    );
    Ok(())
}

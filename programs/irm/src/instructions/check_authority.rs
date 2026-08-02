use anchor_lang::prelude::*;
use irm_state::IrmState;

use crate::error::ErrorCode;

/// Assert that `authority` is the key allowed to rewrite this curve.
///
/// Part of the IRM ABI rather than something a consumer reads off the account,
/// because consumers do not know this program's layout: a market records which
/// program serves as its IRM and CPIs it, so any program implementing
/// `borrow_rate` and this method can back a market. Mirrors `guard::check` —
/// same shape of "ask the owning program to assert something about a key".
///
/// `calma::create` is the caller. `irm::initialize` is permissionless and
/// first-come-first-served on the `["irm_config", pool]` PDA, so whoever claims
/// it names themselves the authority for the life of the market; without this
/// check a market could be stood up already bound to a rate curve someone else
/// controls, with nothing on `Pool` revealing it.
#[derive(Accounts)]
pub struct CheckAuthority<'info> {
    /// Seed-derived so the account is provably the canonical
    /// `["irm_config", pool]` PDA and not some other account carrying a forged
    /// `authority` field.
    #[account(
        seeds = [b"irm_config", pool.key().as_ref()],
        bump = irm_state.load()?.bump,
        has_one = pool,
    )]
    pub irm_state: AccountLoader<'info, IrmState>,
    /// CHECK: verified via the has_one constraint on irm_state
    pub pool: UncheckedAccount<'info>,
}

pub(crate) fn handler(ctx: Context<CheckAuthority>, authority: Pubkey) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.irm_state.load()?.authority,
        authority,
        ErrorCode::Unauthorized
    );
    Ok(())
}

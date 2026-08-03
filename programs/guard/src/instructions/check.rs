use crate::{error::ErrorCode, state::GuardState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Check<'info> {
    /// Re-derived from the authority the account itself records, so whatever is
    /// passed here is provably the canonical `["guard", authority]` PDA and not
    /// some other program-owned account with a forged `authority` field.
    ///
    /// This does **not** tell the caller *whose* list it is — guards are
    /// per-authority and permissionless to create, so the consumer must pin the
    /// exact address it expects. `calma` compares against `Pool::guard_state`,
    /// fixed at market creation.
    #[account(
        seeds = [crate::GUARD_SEED.as_bytes(), guard_state.authority.as_ref()],
        bump = guard_state.bump,
    )]
    pub guard_state: Account<'info, GuardState>,
}

pub fn check_handler(ctx: Context<Check>, pubkey: Pubkey) -> Result<()> {
    require!(
        ctx.accounts.guard_state.whitelist.contains(&pubkey),
        ErrorCode::NotWhitelisted
    );
    Ok(())
}

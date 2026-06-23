use crate::{error::ErrorCode, state::GuardState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Remove<'info> {
    #[account(mut, has_one = authority)]
    pub guard_state: Account<'info, GuardState>,
    pub authority: Signer<'info>,
}

pub fn remove_handler(ctx: Context<Remove>, pubkey: Pubkey) -> Result<()> {
    let guard_state = &mut ctx.accounts.guard_state;
    let pos = guard_state
        .whitelist
        .iter()
        .position(|p| p == &pubkey)
        .ok_or(ErrorCode::NotWhitelisted)?;
    guard_state.whitelist.remove(pos);
    Ok(())
}

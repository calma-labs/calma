use crate::{error::ErrorCode, state::GuardState, MAX_WHITELIST};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Add<'info> {
    #[account(mut, has_one = authority)]
    pub guard_state: Account<'info, GuardState>,
    pub authority: Signer<'info>,
}

pub fn add_handler(ctx: Context<Add>, pubkey: Pubkey) -> Result<()> {
    let guard_state = &mut ctx.accounts.guard_state;
    require!(guard_state.whitelist.len() < MAX_WHITELIST, ErrorCode::WhitelistFull);
    require!(!guard_state.whitelist.contains(&pubkey), ErrorCode::AlreadyWhitelisted);
    guard_state.whitelist.push(pubkey);
    Ok(())
}

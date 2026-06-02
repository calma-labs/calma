use crate::{error::ErrorCode, state::GuardState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Check<'info> {
    pub guard_state: Account<'info, GuardState>,
}

pub fn check_handler(ctx: Context<Check>, pubkey: Pubkey) -> Result<()> {
    require!(
        ctx.accounts.guard_state.whitelist.contains(&pubkey),
        ErrorCode::NotWhitelisted
    );
    Ok(())
}

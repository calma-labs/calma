use crate::state::GuardState;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Create<'info> {
    #[account(
        init,
        payer = payer,
        space = GuardState::SPACE,
        seeds = [b"guard", authority.key().as_ref()],
        bump
    )]
    pub guard_state: Account<'info, GuardState>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn create_handler(ctx: Context<Create>) -> Result<()> {
    let guard_state = &mut ctx.accounts.guard_state;
    guard_state.authority = ctx.accounts.authority.key();
    guard_state.whitelist = Vec::new();
    guard_state.bump = ctx.bumps.guard_state;
    Ok(())
}

use crate::state::GuardState;
use anchor_lang::prelude::*;

/// Creates a whitelist owned by `authority`.
///
/// The PDA is seeded on `["guard", authority]`, so one authority owns exactly
/// one list and different authorities maintain different subsets — a market
/// gated on institutional LPs and one gated on a testing cohort are separate
/// accounts with separate owners.
///
/// This is only safe because consumers **pin the state account, not the
/// program**. Creation is permissionless, so anyone can stand up a list naming
/// themselves and add themselves to it; a consumer that merely checked the
/// caller passed *a* guard account owned by *this* program would gate nothing.
/// `calma` records the exact `guard_state` in `Pool` at market creation and
/// compares every later check against that stored address — see
/// `state::Pool::guard_state`. A consumer that cannot pin an address must not
/// use this program.
#[derive(Accounts)]
pub struct Create<'info> {
    #[account(
        init,
        payer = payer,
        space = GuardState::SPACE,
        seeds = [crate::GUARD_SEED.as_bytes(), authority.key().as_ref()],
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

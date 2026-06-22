use anchor_lang::prelude::*;
use irm_state::{IrmState, DEFAULT_POOL_FEE_BPS};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<IrmState>(),
        seeds = [b"irm_config", pool.key().as_ref()],
        bump,
    )]
    pub irm_config: AccountLoader<'info, IrmState>,
    /// CHECK: pool is used only as a seed for PDA derivation
    pub pool: UncheckedAccount<'info>,
    /// The authority that will be allowed to update the fee curve.
    pub authority: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub(crate) fn handler(ctx: Context<Initialize>) -> Result<()> {
    let mut config = ctx.accounts.irm_config.load_init()?;
    config.pool = ctx.accounts.pool.key();
    config.authority = ctx.accounts.authority.key();
    config.model.curves[0].b = DEFAULT_POOL_FEE_BPS;
    config.model.curves[0].enabled = 1;
    config.bump = ctx.bumps.irm_config;
    Ok(())
}

use anchor_lang::prelude::*;

use crate::state::IrmConfig;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<IrmConfig>(),
        seeds = [b"irm_config", pool.key().as_ref()],
        bump,
    )]
    pub irm_config: AccountLoader<'info, IrmConfig>,
    /// CHECK: pool is used only as a seed for PDA derivation
    pub pool: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Initialize>, a: u64, b: u64) -> Result<()> {
    let mut config = ctx.accounts.irm_config.load_init()?;
    config.pool = ctx.accounts.pool.key();
    config.a = a;
    config.b = b;
    config.bump = ctx.bumps.irm_config;
    Ok(())
}

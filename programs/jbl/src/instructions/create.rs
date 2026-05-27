use crate::state::Pool;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[constant]
pub const POOL_SPACE: u64 = (8 + std::mem::size_of::<Pool>()) as u64;

#[derive(Accounts)]
pub struct Create<'info> {
    /// The pool data account.  Must be pre-allocated (size = POOL_SPACE) and
    /// owned by this program before calling `create`.  Pre-allocating in a
    /// separate transaction bypasses the 10 KB CPI account-creation limit.
    #[account(zero)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; used as authority for vault token accounts and LP mint.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// Token vault for holding deposited collateral tokens.
    #[account(
        init,
        payer = payer,
        token::mint = collateral_mint,
        token::authority = state,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// Token vault for holding deposited lend tokens.
    #[account(
        init,
        payer = payer,
        token::mint = lend_mint,
        token::authority = state,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The LP token mint for the lend side of this pool.
    #[account(
        init,
        payer = payer,
        mint::decimals = lend_mint.decimals,
        mint::authority = state,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump
    )]
    pub lp_mint: Account<'info, Mint>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The lend token mint (deposited by lenders; borrowed by borrowers).
    pub lend_mint: Account<'info, Mint>,

    /// The authority that will control this pool.
    pub authority: Signer<'info>,

    /// The account that pays for account creation.
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn create_handler(
    ctx: Context<Create>,
    ltv_percent: u8,
    rate_program: Pubkey,
    rate_state: Pubkey,
) -> Result<()> {
    let mut pool = ctx.accounts.pool.load_init()?;

    pool.authority = ctx.accounts.authority.key();
    pool.collateral_mint = ctx.accounts.collateral_mint.key();
    pool.lend_mint = ctx.accounts.lend_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.total_collateral_deposited = 0;
    pool.market.total_supply_assets = 0;
    pool.market.total_supply_shares = 0;
    pool.market.total_borrow_assets = 0;
    pool.market.total_borrow_shares = 0;
    pool.market.last_update = Clock::get()?.unix_timestamp;
    pool.market.fee = 0;
    pool.market.assets_in_queue = 0;
    pool.ltv_percent = ltv_percent;
    pool.rate_program = rate_program;
    pool.rate_state = rate_state;
    pool.lp_mint_bump = ctx.bumps.lp_mint;
    // withdrawal_queue is zero-initialised by load_init (head=0, tail=0)

    msg!(
        "Created pool for authority: {} collateral_mint: {} lend_mint: {} lp_mint: {} at slot: {}",
        ctx.accounts.authority.key(),
        ctx.accounts.collateral_mint.key(),
        ctx.accounts.lend_mint.key(),
        ctx.accounts.lp_mint.key(),
        Clock::get()?.slot
    );

    Ok(())
}

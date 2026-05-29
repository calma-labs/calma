use crate::oracle::OracleState;
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The lend token mint (token being repaid).
    pub lend_mint: Account<'info, Mint>,

    /// The borrower
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The borrower's lend-token account (source of repayment)
    #[account(
        mut,
        associated_token::mint = lend_mint,
        associated_token::authority = authority,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault (destination of repayment)
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The user's position — borrow fields are reset on successful repay
    #[account(
        mut,
        seeds = [b"user_position", pool.key().as_ref(), authority.key().as_ref()],
        bump = user_position.load()?.bump,
        constraint = user_position.load()?.authority == authority.key(),
        constraint = user_position.load()?.pool == pool.key()
            @ crate::error::ErrorCode::NoBorrowFound,
        constraint = user_position.load()?.debt_shares > 0
            @ crate::error::ErrorCode::NoBorrowFound,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.rate_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn repay_handler<'a>(ctx: Context<'a, Repay<'a>>, amount: u64) -> Result<()> {
    // ── 1. Accrue interest on the pool via IRM CPI ────────────────────────────
    let (oracle, utilization) = {
        let pool = ctx.accounts.pool.load()?;
        let oracle = OracleState::new(&ctx.accounts.sysvar_instructions.to_account_info(), pool.feed_program, pool.feed_state)?;
        (oracle, pool.calculate_utilization())
    };
    let irm = crate::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    let (repay_amount, shares_to_burn) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = jbl_math::Core::new(pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?);
        core.accrue_interest().ok_or(crate::error::ErrorCode::MathOverflow)?;
        let result = core.repay(amount).ok_or(crate::error::ErrorCode::MathOverflow)?;
        require!(ctx.accounts.user_token_account.amount >= result.0, crate::error::ErrorCode::InsufficientFunds);
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.debt_shares = core.position.debt_shares;
        result
    };

    // ── 3. Transfer lend tokens back to the lend vault ─────────────────────────
    anchor_spl::token::transfer(
        CpiContext::new(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.lend_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        repay_amount,
    )?;

    let pool = ctx.accounts.pool.load()?;
    msg!(
        "Repaid {} tokens ({} shares). Pool total_borrow_assets: {}, total_shares: {}",
        repay_amount,
        shares_to_burn,
        pool.market.total_borrow_assets,
        pool.market.total_borrow_shares,
    );

    Ok(())
}

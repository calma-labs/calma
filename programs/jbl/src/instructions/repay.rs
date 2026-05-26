use crate::math::{amount_to_shares_burned, shares_to_amount};
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

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

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn repay_handler(ctx: Context<Repay>, amount: u64) -> Result<()> {
    let current_ts = Clock::get()?.unix_timestamp;

    // ── 1. Accrue interest on the pool ────────────────────────────────────────
    ctx.accounts.pool.load_mut()?.accrue_interest(current_ts, ctx.remaining_accounts)?;

    // ── 2. Compute exact amount owed and shares to burn ───────────────────────
    let (repay_amount, shares_to_burn) = {
        let pool = ctx.accounts.pool.load()?;
        let position = ctx.accounts.user_position.load()?;
        let debt_shares = position.debt_shares;
        let total_due =
            shares_to_amount(debt_shares, pool.market.total_borrow_assets, pool.market.total_borrow_shares)
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let repay_amount = amount.min(total_due);
        require!(
            ctx.accounts.user_token_account.amount >= repay_amount,
            crate::error::ErrorCode::InsufficientFunds
        );
        // If repaying the full debt, burn ALL shares to avoid rounding dust
        let shares_to_burn = if repay_amount == total_due {
            debt_shares
        } else {
            amount_to_shares_burned(
                repay_amount,
                pool.market.total_borrow_assets,
                pool.market.total_borrow_shares,
                debt_shares,
            )
            .ok_or(crate::error::ErrorCode::MathOverflow)?
        };
        (repay_amount, shares_to_burn)
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

    // ── 4. Update pool and position state ─────────────────────────────────────
    let (total_borrow_assets, total_borrow_shares) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_borrow_shares = pool
            .market
            .total_borrow_shares
            .checked_sub(shares_to_burn)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        pool.market.total_borrow_assets = pool
            .market
            .total_borrow_assets
            .checked_sub(repay_amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        (pool.market.total_borrow_assets, pool.market.total_borrow_shares)
    };

    {
        let mut position = ctx.accounts.user_position.load_mut()?;
        position.debt_shares = position
            .debt_shares
            .checked_sub(shares_to_burn)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
    }

    msg!(
        "Repaid {} tokens ({} shares). Pool total_borrow_assets: {}, total_shares: {}",
        repay_amount,
        shares_to_burn,
        total_borrow_assets,
        total_borrow_shares,
    );

    Ok(())
}

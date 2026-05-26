use crate::math::{amount_to_shares, shares_to_amount};
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct Borrow<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs lend-vault-transfer CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint (token being borrowed).
    pub lend_mint: Account<'info, Mint>,

    /// The borrower
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's lend-token account (destination) — created if it doesn't exist yet
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = lend_mint,
        associated_token::authority = authority,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault (source of borrowed tokens)
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key(),
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The user's position — collateral balance and active borrow fields.
    #[account(
        mut,
        seeds = [b"user_position", pool.key().as_ref(), authority.key().as_ref()],
        bump = user_position.load()?.bump,
        constraint = user_position.load()?.authority == authority.key(),
        constraint = user_position.load()?.pool == pool.key()
            @ crate::error::ErrorCode::InvalidAmount,
        constraint = user_position.load()?.collateral_deposited > 0
            @ crate::error::ErrorCode::InsufficientFunds,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn borrow_handler(ctx: Context<Borrow>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);
    require!(
        ctx.accounts.lend_vault.amount >= amount,
        crate::error::ErrorCode::InsufficientFunds
    );

    // ── 1. Accrue interest on the pool ────────────────────────────────────────
    let current_ts = Clock::get()?.unix_timestamp;
    let irm_rate = crate::irm::fetch_irm_rate(&*ctx.accounts.pool.load()?, ctx.remaining_accounts)?;
    ctx.accounts.pool.load_mut()?.accrue_interest(current_ts, irm_rate)?;

    // ── 2. LTV check and share calculation ───────────────────────────────────
    let new_shares = {
        let pool = ctx.accounts.pool.load()?;
        let position = ctx.accounts.user_position.load()?;
        let collateral = position.collateral_deposited;
        let max_borrowable = collateral
            .checked_mul(pool.ltv_percent as u64)
            .ok_or(crate::error::ErrorCode::MathOverflow)?
            .checked_div(100)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;

        let current_debt = if pool.market.total_borrow_shares > 0 {
            shares_to_amount(
                position.debt_shares,
                pool.market.total_borrow_assets,
                pool.market.total_borrow_shares,
            )
            .ok_or(crate::error::ErrorCode::MathOverflow)?
        } else {
            0
        };

        let available = max_borrowable
            .checked_sub(current_debt)
            .ok_or(crate::error::ErrorCode::InsufficientFunds)?;
        require!(amount <= available, crate::error::ErrorCode::InsufficientFunds);

        let new_shares = amount_to_shares(amount, pool.market.total_borrow_assets, pool.market.total_borrow_shares)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        require!(new_shares > 0, crate::error::ErrorCode::InvalidAmount);

        new_shares
    };

    // ── 3. Update user position ───────────────────────────────────────────────
    {
        let mut position = ctx.accounts.user_position.load_mut()?;
        position.debt_shares = position
            .debt_shares
            .checked_add(new_shares)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
    }

    // ── 4. Transfer lend tokens to the borrower ───────────────────────────────
    let seeds = &[b"state" as &[u8], &[ctx.bumps.state]];
    let signer = &[&seeds[..]];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.lend_vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.state.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;

    // ── 5. Update pool state ──────────────────────────────────────────────────
    let (total_borrow_assets, total_borrow_shares) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_borrow_shares = pool
            .market
            .total_borrow_shares
            .checked_add(new_shares)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        pool.market.total_borrow_assets = pool
            .market
            .total_borrow_assets
            .checked_add(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        (pool.market.total_borrow_assets, pool.market.total_borrow_shares)
    };

    msg!(
        "Borrowed {} lend tokens → {} shares. Pool total_borrow_assets: {}, total_shares: {}",
        amount,
        new_shares,
        total_borrow_assets,
        total_borrow_shares,
    );

    Ok(())
}

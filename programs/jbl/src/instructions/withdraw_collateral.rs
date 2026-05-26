use crate::math::shares_to_amount;
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct WithdrawCollateral<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs collateral-vault-transfer CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The withdrawer
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's collateral token account (destination)
    #[account(
        mut,
        constraint = user_token_account.owner == authority.key(),
        constraint = user_token_account.mint == collateral_mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's collateral vault (source)
    #[account(
        mut,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump,
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// PDA that records the user's collateral deposit and borrow position.
    #[account(
        mut,
        seeds = [b"user_position", pool.key().as_ref(), authority.key().as_ref()],
        bump = user_position.load()?.bump,
        constraint = user_position.load()?.authority == authority.key(),
        constraint = user_position.load()?.pool == pool.key()
            @ crate::error::ErrorCode::InvalidAmount,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn withdraw_collateral_handler(ctx: Context<WithdrawCollateral>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);
    {
        let position = ctx.accounts.user_position.load()?;
        require!(
            amount <= position.collateral_deposited,
            crate::error::ErrorCode::InsufficientFunds
        );
    }

    // ── 1. Accrue interest on the pool ────────────────────────────────────────
    let current_ts = Clock::get()?.unix_timestamp;
    let irm_rate = crate::irm::fetch_irm_rate(&*ctx.accounts.pool.load()?, ctx.remaining_accounts)?;
    ctx.accounts.pool.load_mut()?.accrue_interest(current_ts, irm_rate)?;

    // ── 2. LTV check: ensure remaining collateral still covers open debt ──────
    {
        let pool = ctx.accounts.pool.load()?;
        let position = ctx.accounts.user_position.load()?;

        let remaining_collateral = position
            .collateral_deposited
            .checked_sub(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let max_borrowable = remaining_collateral
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
        require!(
            current_debt <= max_borrowable,
            crate::error::ErrorCode::InsufficientFunds
        );
    }

    // ── 3. Check collateral vault liquidity ───────────────────────────────────
    require!(
        ctx.accounts.collateral_vault.amount >= amount,
        crate::error::ErrorCode::InsufficientFunds
    );

    // ── 4. Transfer collateral tokens back to user ────────────────────────────
    let seeds = &[b"state" as &[u8], &[ctx.bumps.state]];
    let signer = &[&seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info().key.clone(),
            Transfer {
                from: ctx.accounts.collateral_vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.state.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;

    // ── 5. Update state ───────────────────────────────────────────────────────
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.total_collateral_deposited = pool
            .total_collateral_deposited
            .checked_sub(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
    }

    let remaining = {
        let mut position = ctx.accounts.user_position.load_mut()?;
        position.collateral_deposited = position
            .collateral_deposited
            .checked_sub(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        position.collateral_deposited
    };

    msg!(
        "Withdrew {} collateral tokens. Remaining collateral deposit: {}",
        amount,
        remaining,
    );

    Ok(())
}

use crate::math::shares_to_amount;
use crate::oracle::OracleState;
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.rate_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    /// CHECK: feed state — price is read from its `value` field; key validated against pool.feed_state.
    #[account(constraint = feed_state.key() == pool.load()?.feed_state @ crate::error::ErrorCode::InvalidAmount)]
    pub feed_state: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn withdraw_collateral_handler<'a>(ctx: Context<'a, WithdrawCollateral<'a>>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);
    {
        let position = ctx.accounts.user_position.load()?;
        require!(
            amount <= position.collateral_deposited,
            crate::error::ErrorCode::InsufficientFunds
        );
    }

    // ── 1. Accrue interest on the pool ────────────────────────────────────────
    let (oracle, utilization) = {
        let pool = ctx.accounts.pool.load()?;
        let oracle = OracleState::new(&ctx.accounts.sysvar_instructions.to_account_info(), pool.feed_program, pool.feed_state)?;
        (oracle, pool.calculate_utilization())
    };
    let irm = crate::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    ctx.accounts.pool.load_mut()?.accrue_interest(&irm, &oracle)?;

    // ── 2. LTV check: ensure remaining collateral still covers open debt ──────
    {
        let pool = ctx.accounts.pool.load()?;
        let position = ctx.accounts.user_position.load()?;

        let remaining_collateral = position
            .collateral_deposited
            .checked_sub(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let oracle_price = crate::oracle::read_feed_price(&ctx.accounts.feed_state.to_account_info())?;
        let max_borrowable_u128 = (remaining_collateral as u128)
            .checked_mul(oracle_price as u128)
            .ok_or(crate::error::ErrorCode::MathOverflow)?
            .checked_div(crate::oracle::PRICE_SCALE)
            .ok_or(crate::error::ErrorCode::MathOverflow)?
            .checked_mul(pool.market.ltv_percent as u128)
            .ok_or(crate::error::ErrorCode::MathOverflow)?
            .checked_div(100)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let max_borrowable = u64::try_from(max_borrowable_u128)
            .map_err(|_| crate::error::ErrorCode::MathOverflow)?;
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

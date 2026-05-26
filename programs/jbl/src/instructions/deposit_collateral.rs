use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct DepositCollateral<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The depositor
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's collateral token account (source)
    #[account(
        mut,
        constraint = user_token_account.owner == authority.key(),
        constraint = user_token_account.mint == collateral_mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's collateral vault (destination)
    #[account(
        mut,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump,
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// PDA that records the user's collateral deposit and borrow position.
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + 88, // UserPosition: 32+32+8+8+1+7 = 88
        seeds = [b"user_position", pool.key().as_ref(), authority.key().as_ref()],
        bump,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn deposit_collateral_handler(ctx: Context<DepositCollateral>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);

    // Validate the mint matches the pool's collateral_mint.
    {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.collateral_mint.key() == pool.collateral_mint,
            crate::error::ErrorCode::InvalidAmount
        );
    }

    // Transfer collateral tokens from user to the pool's collateral vault.
    anchor_spl::token::transfer(
        CpiContext::new(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.collateral_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    // Update pool's raw collateral total.
    let total_collateral = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.total_collateral_deposited = pool
            .total_collateral_deposited
            .checked_add(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        pool.total_collateral_deposited
    };

    // Initialize or accumulate into the user position PDA.
    {
        let needs_init = {
            let account_info = ctx.accounts.user_position.to_account_info();
            let data = account_info.try_borrow_data()?;
            data.len() >= 8 && data[..8].iter().all(|&b| b == 0)
        };
        if needs_init {
            let mut position = ctx.accounts.user_position.load_init()?;
            position.authority = ctx.accounts.authority.key();
            position.pool = ctx.accounts.pool.key();
            position.collateral_deposited = amount;
            position.debt_shares = 0;
            position.bump = ctx.bumps.user_position;
        } else {
            let mut position = ctx.accounts.user_position.load_mut()?;
            position.collateral_deposited = position
                .collateral_deposited
                .checked_add(amount)
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
        }
    }

    msg!(
        "Deposited {} collateral tokens. Total collateral in pool: {}",
        amount,
        total_collateral,
    );

    Ok(())
}

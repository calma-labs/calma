use crate::state::Pool;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, MintTo, Token, TokenAccount};

#[derive(Accounts)]
pub struct DepositLent<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs LP-mint CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint accepted by this pool.
    pub lend_mint: Account<'info, Mint>,

    /// The LP token mint for this lending pool.
    #[account(
        mut,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump,
    )]
    pub lp_mint: Account<'info, Mint>,

    /// The depositor.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's lend-token source account.
    #[account(
        mut,
        constraint = user_lend_token_account.owner == authority.key()
            @ crate::error::ErrorCode::InvalidAmount,
        constraint = user_lend_token_account.mint == lend_mint.key()
            @ crate::error::ErrorCode::InvalidAmount,
    )]
    pub user_lend_token_account: Account<'info, TokenAccount>,

    /// The user's LP token account (destination).
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = lp_mint,
        associated_token::authority = authority,
    )]
    pub user_lp_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault — holds deposited lend tokens.
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key()
            @ crate::error::ErrorCode::InvalidAmount,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn deposit_lent_handler(ctx: Context<DepositLent>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);

    // Validate lend_mint matches what is stored in the pool.
    {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.lend_mint.key() == pool.lend_mint,
            crate::error::ErrorCode::InvalidAmount
        );
    }

    // ── 1. Calculate LP tokens to mint (proportional to existing deposits) ────
    // Uses total_lend_deposited as the denominator so LP value is consistent
    // with the deposit accounting in the pool.
    let lp_to_mint = {
        let pool = ctx.accounts.pool.load()?;

        if pool.market.total_supply_shares == 0 || pool.market.total_supply_assets == 0 {
            // First lend deposit: 1 LP per token.
            amount
        } else {
            (amount as u128)
                .checked_mul(pool.market.total_supply_shares as u128)
                .ok_or(crate::error::ErrorCode::MathOverflow)?
                .checked_div(pool.market.total_supply_assets as u128)
                .ok_or(crate::error::ErrorCode::MathOverflow)? as u64
        }
    };

    require!(lp_to_mint > 0, crate::error::ErrorCode::InvalidAmount);

    // ── 2. Transfer lend tokens from user to the pool's lend vault ────────────
    anchor_spl::token::transfer(
        CpiContext::new(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_lend_token_account.to_account_info(),
                to: ctx.accounts.lend_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    // ── 3. Mint LP tokens directly to the user's wallet ──────────────────────
    let state_bump = ctx.bumps.state;
    let seeds = &[b"state" as &[u8], &[state_bump]];
    let signer = &[&seeds[..]];

    anchor_spl::token::mint_to(
        CpiContext::new_with_signer(
            *ctx.accounts.token_program.to_account_info().key,
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.user_lp_token_account.to_account_info(),
                authority: ctx.accounts.state.to_account_info(),
            },
            signer,
        ),
        lp_to_mint,
    )?;

    // ── 4. Update pool lend totals ────────────────────────────────────────────
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_supply_assets = pool
            .market
            .total_supply_assets
            .checked_add(amount)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        pool.market.total_supply_shares = pool
            .market
            .total_supply_shares
            .checked_add(lp_to_mint)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;

        msg!(
            "DepositLent: deposited {} lend tokens, minted {} LP tokens. total_supply_assets: {}, total_supply_shares: {}",
            amount,
            lp_to_mint,
            pool.market.total_supply_assets,
            pool.market.total_supply_shares,
        );
    }

    Ok(())
}

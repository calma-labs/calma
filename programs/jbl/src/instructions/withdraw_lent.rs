use crate::{state::Pool, withdrawal_queue::WithdrawalQueueEntry};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Burn, Mint, Token, TokenAccount},
};

#[derive(Accounts)]
pub struct WithdrawLent<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs lend-vault-transfer CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint accepted by this pool.
    pub lend_mint: Account<'info, Mint>,

    /// The pool's LP token mint.
    #[account(
        mut,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump,
    )]
    pub lp_mint: Account<'info, Mint>,

    /// The withdrawer.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's LP token account (source for burning).
    #[account(
        mut,
        associated_token::mint = lp_mint,
        associated_token::authority = authority,
    )]
    pub user_lp_token_account: Account<'info, TokenAccount>,

    /// The user's lend-token account (destination) — created if it doesn't exist.
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = lend_mint,
        associated_token::authority = authority,
    )]
    pub user_lend_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault — holds lend tokens; source for withdrawal.
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

pub fn withdraw_lent_handler(ctx: Context<WithdrawLent>, shares: u64) -> Result<()> {
    require!(shares > 0, crate::error::ErrorCode::InvalidAmount);
    require!(
        ctx.accounts.user_lp_token_account.amount >= shares,
        crate::error::ErrorCode::InsufficientFunds
    );

    // Validate lend_mint matches what is stored in the pool.
    {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.lend_mint.key() == pool.lend_mint,
            crate::error::ErrorCode::InvalidAmount
        );
        require!(
            pool.market.total_supply_shares > 0,
            crate::error::ErrorCode::InvalidAmount
        );
    }

    // ── 1. Burn LP tokens from user (always) ─────────────────────────────────
    // Burning here prevents double-use while the request sits in the queue.
    // `total_supply_shares` is NOT decremented yet — queued shares keep accruing
    // proportional value relative to the lend vault balance until processed.
    anchor_spl::token::burn(
        CpiContext::new(
            *ctx.accounts.token_program.to_account_info().key,
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.user_lp_token_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        shares,
    )?;

    // ── 2. Decide: immediate withdrawal or enqueue ────────────────────────────
    // We go immediate only when the queue is empty AND the lend vault holds
    // enough tokens to cover the shares' full proportional value (based on
    // total_supply_assets, not just the current vault balance).
    let vault_balance = ctx.accounts.lend_vault.amount;

    let immediate = {
        let pool = ctx.accounts.pool.load()?;
        let queue_is_empty = pool.withdrawal_queue.head == pool.withdrawal_queue.tail;
        // Proportional lend tokens for `shares` based on full deposited amount.
        let lend_for_shares = (shares as u128)
            .checked_mul(pool.market.total_supply_assets as u128)
            .ok_or(crate::error::ErrorCode::MathOverflow)?
            .checked_div(pool.market.total_supply_shares as u128)
            .ok_or(crate::error::ErrorCode::MathOverflow)? as u64;
        queue_is_empty && vault_balance >= lend_for_shares && lend_for_shares > 0
    };

    if immediate {
        // ── 3a. Immediate: convert LP → lend tokens and transfer ─────────────
        let withdraw_amount = {
            let pool = ctx.accounts.pool.load()?;
            (shares as u128)
                .checked_mul(pool.market.total_supply_assets as u128)
                .ok_or(crate::error::ErrorCode::MathOverflow)?
                .checked_div(pool.market.total_supply_shares as u128)
                .ok_or(crate::error::ErrorCode::MathOverflow)? as u64
        };

        // Decrement total_supply_shares and total_supply_assets on immediate path.
        {
            let mut pool = ctx.accounts.pool.load_mut()?;
            pool.market.total_supply_shares = pool
                .market
                .total_supply_shares
                .checked_sub(shares)
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
            pool.market.total_supply_assets = pool
                .market
                .total_supply_assets
                .checked_sub(withdraw_amount.min(pool.market.total_supply_assets))
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
        }

        let state_bump = ctx.bumps.state;
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];

        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *ctx.accounts.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: ctx.accounts.lend_vault.to_account_info(),
                    to: ctx.accounts.user_lend_token_account.to_account_info(),
                    authority: ctx.accounts.state.to_account_info(),
                },
                signer,
            ),
            withdraw_amount,
        )?;

        msg!(
            "Leave: burned {} LP, withdrew {} lend tokens immediately. total_supply_shares: {}",
            shares,
            withdraw_amount,
            ctx.accounts.pool.load()?.market.total_supply_shares,
        );
    } else {
        // ── 3b. Queued: store shares in queue; totals unchanged ───────────────
        // Conversion to lend tokens happens when the queue entry is processed,
        // so the shares continue to reflect their accrued value at that time.
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.withdrawal_queue.push(WithdrawalQueueEntry {
            requester: ctx.accounts.authority.key(),
            amount: shares,
        })?;
        msg!(
            "Leave: burned {} LP, enqueued for later withdrawal. total_supply_shares: {}",
            shares,
            pool.market.total_supply_shares,
        );
    }

    Ok(())
}

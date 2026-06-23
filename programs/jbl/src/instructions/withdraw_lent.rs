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

impl<'info> WithdrawLent<'info> {
    pub fn burn_lp(&self, shares: u64) -> Result<()> {
        anchor_spl::token::burn(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                Burn {
                    mint: self.lp_mint.to_account_info(),
                    from: self.user_lp_token_account.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            shares,
        )
    }

    pub fn transfer_lend_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.lend_vault.to_account_info(),
                    to: self.user_lend_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
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
    ctx.accounts.burn_lp(shares)?;

    // ── 2. Compute token amount and update market ─────────────────────────────
    let vault_balance = ctx.accounts.lend_vault.amount;
    let state_bump = ctx.bumps.state;
    let (immediate, lend_for_shares) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let queue_is_empty = pool.withdrawal_queue.head == pool.withdrawal_queue.tail;
        let mut core = math::Core::new(pool.market);
        let lend_for_shares = core
            .calc_lend_for_shares(shares)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let immediate = queue_is_empty && vault_balance >= lend_for_shares && lend_for_shares > 0;
        if immediate {
            core.withdraw_lent_immediate(shares, |amt| {
                ctx.accounts.transfer_lend_to_user(amt, state_bump)
            })
            .map_err(crate::error::ErrorCode::from)?;
        } else {
            core.withdraw_lent_queued(shares)
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
            pool.withdrawal_queue.push(WithdrawalQueueEntry::new(
                ctx.accounts.authority.key(),
                lend_for_shares,
            ))?;
        }
        pool.market = core.market;
        (immediate, lend_for_shares)
    };

    if immediate {
        msg!(
            "Leave: burned {} LP, withdrew {} lend tokens immediately. total_supply_shares: {}",
            shares,
            lend_for_shares,
            ctx.accounts.pool.load()?.market.total_supply_shares,
        );
    } else {
        msg!(
            "Leave: burned {} LP, enqueued {} lend tokens for later withdrawal. total_supply_shares: {}",
            shares,
            lend_for_shares,
            ctx.accounts.pool.load()?.market.total_supply_shares,
        );
    }

    Ok(())
}

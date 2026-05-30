use crate::oracle::OracleState;
use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.irm_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    /// CHECK: feed state — price is read from its `value` field; key validated against pool.feed_state.
    #[account(constraint = feed_state.key() == pool.load()?.feed_state @ crate::error::ErrorCode::InvalidAmount)]
    pub feed_state: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Borrow<'info> {
    pub fn transfer_lend_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.lend_vault.to_account_info(),
                    to: self.user_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn borrow_handler<'a>(ctx: Context<'a, Borrow<'a>>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);
    require!(
        ctx.accounts.lend_vault.amount >= amount,
        crate::error::ErrorCode::InsufficientFunds
    );

    // ── 1. Accrue interest on the pool via IRM CPI ────────────────────────────
    let (oracle, utilization) = {
        let pool = ctx.accounts.pool.load()?;
        let oracle = OracleState::new(&ctx.accounts.sysvar_instructions.to_account_info(), pool.feed_program, pool.feed_state)?;
        (oracle, pool.calculate_utilization())
    };
    let oracle_price = crate::oracle::read_feed_price(&ctx.accounts.feed_state.to_account_info())?;
    let irm = crate::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    let state_bump = ctx.bumps.state;
    let new_shares = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = jbl_math::Core::new(pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?);
        core.accrue_interest().ok_or(crate::error::ErrorCode::MathOverflow)?;
        let new_shares = core.borrow(amount, oracle_price, |amt| ctx.accounts.transfer_lend_to_user(amt, state_bump))
            .map_err(|e| match e {
                jbl_math::MathError::Overflow => crate::error::ErrorCode::InsufficientFunds.into(),
                jbl_math::MathError::Transfer(e) => e,
            })?;
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.debt_shares = core.position.debt_shares;
        new_shares
    };

    let pool = ctx.accounts.pool.load()?;
    msg!(
        "Borrowed {} lend tokens → {} shares. Pool total_borrow_assets: {}, total_shares: {}",
        amount,
        new_shares,
        pool.market.total_borrow_assets,
        pool.market.total_borrow_shares,
    );

    Ok(())
}

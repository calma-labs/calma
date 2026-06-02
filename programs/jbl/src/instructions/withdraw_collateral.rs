use crate::hooks::oracle::OracleState;
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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.rate_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: feed program — invoked via CPI to read the oracle price.
    #[account(constraint = feed_program.key() == pool.load()?.feed_program @ crate::error::ErrorCode::InvalidAmount)]
    pub feed_program: UncheckedAccount<'info>,

    /// CHECK: feed state — price is fetched via CPI; key validated against pool.feed_state.
    #[account(constraint = feed_state.key() == pool.load()?.feed_state @ crate::error::ErrorCode::InvalidAmount)]
    pub feed_state: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawCollateral<'info> {
    pub fn transfer_collateral_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        token::transfer(
            CpiContext::new_with_signer(
                self.token_program.to_account_info().key.clone(),
                Transfer {
                    from: self.collateral_vault.to_account_info(),
                    to: self.user_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
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
    let utilization = ctx.accounts.pool.load()?.calculate_utilization();
    let oracle = OracleState::new(ctx.accounts.feed_program.to_account_info(), ctx.accounts.feed_state.to_account_info())?;
    let oracle_price = oracle.price;
    let irm = crate::hooks::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    require!(
        ctx.accounts.collateral_vault.amount >= amount,
        crate::error::ErrorCode::InsufficientFunds
    );

    let state_bump = ctx.bumps.state;
    let remaining = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = jbl_math::Core::new(pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?);
        core.accrue_interest().ok_or(crate::error::ErrorCode::MathOverflow)?;
        let remaining = core.withdraw_collateral(amount, oracle_price, |amt| ctx.accounts.transfer_collateral_to_user(amt, state_bump))
            .map_err(|e| match e {
                jbl_math::MathError::Transfer(e) => e,
                e => crate::error::ErrorCode::from(e).into(),
            })?;
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.collateral_deposited = core.position.collateral_deposited;
        remaining
    };

    msg!(
        "Withdrew {} collateral tokens. Remaining collateral deposit: {}",
        amount,
        remaining,
    );

    Ok(())
}

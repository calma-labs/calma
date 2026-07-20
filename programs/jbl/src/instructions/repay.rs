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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.rate_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Repay<'info> {
    pub fn transfer_lend_to_vault(&self, amount: u64) -> Result<()> {
        anchor_spl::token::transfer(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.user_token_account.to_account_info(),
                    to: self.lend_vault.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
        )
    }
}

pub fn repay_handler<'a>(ctx: Context<'a, Repay<'a>>, amount: u64) -> Result<()> {
    // ── 1. Accrue interest on the pool via IRM CPI ────────────────────────────
    let utilization = ctx.accounts.pool.load()?.calculate_utilization();
    let irm = crate::hooks::irm::IrmState::new(
        ctx.accounts.rate_program.to_account_info(),
        utilization,
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.irm_state.to_account_info(),
    )?;
    let (repay_amount, shares_to_burn) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(pool.market)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::InterestAccrualOverflow)?;
        let result = core
            .repay(amount, |amt| {
                require!(
                    ctx.accounts.user_token_account.amount >= amt,
                    crate::error::ErrorCode::InsufficientFunds
                );
                ctx.accounts.transfer_lend_to_vault(amt)
            })
            .map_err(|e| match e {
                // Preserve the concrete error raised inside the transfer closure
                // (e.g. InsufficientFunds) instead of collapsing it to MathOverflow.
                math::MathError::Transfer(e) => e,
                // The only arithmetic step in `repay` is the debt share→amount
                // valuation; surface it distinctly from interest accrual above.
                math::MathError::Arithmetic => {
                    crate::error::ErrorCode::DebtValuationOverflow.into()
                }
                other => crate::error::ErrorCode::from(other).into(),
            })?;
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.debt_shares = core.position.debt_shares;
        result
    };

    let pool = ctx.accounts.pool.load()?;
    msg!(
        "Repaid {} tokens ({} shares). Pool total_borrow_assets: {}, total_shares: {}",
        repay_amount,
        shares_to_burn,
        pool.market.total_borrow_assets,
        pool.market.total_borrow_shares,
    );

    Ok(())
}

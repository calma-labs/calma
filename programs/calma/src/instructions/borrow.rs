use crate::hooks::oracle::read_feed;
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
        seeds = [::state::seeds::STATE],
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
        seeds = [::state::seeds::LEND_VAULT, pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key(),
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The user's position — collateral balance and active borrow fields.
    #[account(
        mut,
        seeds = [::state::seeds::USER_POSITION, pool.key().as_ref(), authority.key().as_ref()],
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

    /// CHECK: price account, read directly — no CPI. Address is pinned here and
    /// its owner is checked against `pool.feed_program` in the handler; see
    /// `hooks::oracle::read_feed`.
    #[account(constraint = feed_state.key() == pool.load()?.feed_state @ crate::error::ErrorCode::InvalidFeedState)]
    pub feed_state: UncheckedAccount<'info>,

    /// CHECK: required iff `pool.guard_state` is set; validated in the handler.
    pub guard_program: Option<UncheckedAccount<'info>>,

    /// CHECK: required iff `pool.guard_state` is set; must equal it exactly.
    pub guard_state: Option<UncheckedAccount<'info>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Borrow<'info> {
    pub fn transfer_lend_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        crate::instructions::transfer_from_vault(
            *self.token_program.to_account_info().key,
            &self.state.to_account_info(),
            state_bump,
            self.lend_vault.to_account_info(),
            self.user_token_account.to_account_info(),
            amount,
        )
    }
}

pub fn borrow_handler<'a>(ctx: Context<'a, Borrow<'a>>, amount: u64) -> Result<()> {
    // ── 1. Accrue interest on the pool via IRM CPI ────────────────────────────
    //
    // Amount and liquidity rules (including excluding assets reserved for the
    // withdrawal queue) are enforced inside `Core::borrow`.
    let (feed_state_key, feed_program_key) = {
        let pool = ctx.accounts.pool.load()?;
        pool.feed_config()
    };

    // Whitelist gate — entry only. `repay` is deliberately never gated: a
    // borrower removed from the list must always be able to clear their debt.
    // Checked before the oracle/IRM CPIs so a rejected caller costs the minimum.
    crate::hooks::guard::enforce_pool_guard(
        &ctx.accounts.pool,
        &ctx.accounts.guard_program,
        &ctx.accounts.guard_state,
        ctx.accounts.authority.key(),
    )?;

    let vault_balance = ctx.accounts.lend_vault.amount;
    let oracle = read_feed(&ctx.accounts.feed_state, feed_state_key, feed_program_key)?;
    let irm = crate::hooks::irm::IrmState::new(
        ctx.accounts.rate_program.to_account_info(),
        &ctx.accounts.pool,
        ctx.accounts.irm_state.to_account_info(),
    )?;
    let state_bump = ctx.bumps.state;
    let new_shares = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(&mut pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        let new_shares = core
            .borrow(amount, vault_balance, |amt| {
                ctx.accounts.transfer_lend_to_user(amt, state_bump)
            })
            .map_err(|e| match e {
                math::MathError::Transfer(e) => e,
                e => crate::error::ErrorCode::from(e).into(),
            })?;
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

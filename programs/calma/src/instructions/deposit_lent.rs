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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ crate::error::ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.irm_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ crate::error::ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: required iff `pool.guard_state` is set; validated in the handler.
    pub guard_program: Option<UncheckedAccount<'info>>,

    /// CHECK: required iff `pool.guard_state` is set; must equal it exactly.
    pub guard_state: Option<UncheckedAccount<'info>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> DepositLent<'info> {
    pub fn transfer_lend_to_vault(&self, amount: u64) -> Result<()> {
        anchor_spl::token::transfer(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.user_lend_token_account.to_account_info(),
                    to: self.lend_vault.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
        )
    }

    pub fn mint_lp_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                MintTo {
                    mint: self.lp_mint.to_account_info(),
                    to: self.user_lp_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn deposit_lent_handler(ctx: Context<DepositLent>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);

    // Validate lend_mint matches what is stored in the pool, and refuse to price
    // shares while a flash loan is mid-flight (the vault is temporarily short).
    let (utilization, pool_guard_state) = {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.lend_mint.key() == pool.lend_mint,
            crate::error::ErrorCode::InvalidAmount
        );
        require!(
            pool.market.flash_loan_outstanding == 0,
            crate::error::ErrorCode::FlashLoanInProgress
        );
        (pool.calculate_utilization(), pool.guard_state)
    };

    // Whitelist gate — entry only. `withdraw_lent` and `process_queue_entry`
    // are deliberately never gated: a lender removed from the list must still be
    // able to redeem, and the queue must stay drainable for everyone behind them.
    crate::hooks::guard::enforce_pool_guard(
        pool_guard_state,
        &ctx.accounts.guard_program,
        &ctx.accounts.guard_state,
        ctx.accounts.authority.key(),
    )?;

    // ── 1. Accrue interest so LP is minted at the current share price ─────────
    //
    // Without this the depositor buys in at a stale, understated share price and
    // captures a share of interest that accrued before they arrived.
    let irm = crate::hooks::irm::IrmState::new(
        ctx.accounts.rate_program.to_account_info(),
        utilization,
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.irm_state.to_account_info(),
    )?;

    // ── 2. Calculate LP tokens to mint, transfer lend tokens, mint LP ─────────
    let state_bump = ctx.bumps.state;
    let lp_to_mint = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(pool.market)
            .with_irm(irm)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::InterestAccrualOverflow)?;
        let lp = core
            .deposit_lent(
                amount,
                |amt| ctx.accounts.transfer_lend_to_vault(amt),
                |lp| ctx.accounts.mint_lp_to_user(lp, state_bump),
            )
            .map_err(crate::error::ErrorCode::from)?;
        pool.market = core.market;
        lp
    };

    require!(lp_to_mint > 0, crate::error::ErrorCode::InvalidAmount);

    let pool = ctx.accounts.pool.load()?;
    msg!(
        "DepositLent: deposited {} lend tokens, minted {} LP tokens. total_supply_assets: {}, total_supply_shares: {}",
        amount,
        lp_to_mint,
        pool.market.total_supply_assets,
        pool.market.total_supply_shares,
    );

    Ok(())
}

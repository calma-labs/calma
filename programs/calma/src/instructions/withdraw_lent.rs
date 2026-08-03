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
        seeds = [::state::seeds::STATE],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint accepted by this pool.
    pub lend_mint: Account<'info, Mint>,

    /// The pool's LP token mint.
    #[account(
        mut,
        seeds = [::state::seeds::LP_MINT, pool.key().as_ref()],
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
        seeds = [::state::seeds::LEND_VAULT, pool.key().as_ref()],
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
        crate::instructions::transfer_from_vault(
            *self.token_program.to_account_info().key,
            &self.state.to_account_info(),
            state_bump,
            self.lend_vault.to_account_info(),
            self.user_lend_token_account.to_account_info(),
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
    let utilization = {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.lend_mint.key() == pool.lend_mint,
            crate::error::ErrorCode::InvalidAmount
        );
        require!(
            pool.market.total_supply_shares > 0,
            crate::error::ErrorCode::InvalidAmount
        );
        require!(
            pool.market.flash_loan_outstanding == 0,
            crate::error::ErrorCode::FlashLoanInProgress
        );
        pool.calculate_utilization()
    };

    // ── 1. Accrue interest so the exit is priced at the current share price ───
    let irm = crate::hooks::irm::IrmState::new(
        ctx.accounts.rate_program.to_account_info(),
        utilization,
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.irm_state.to_account_info(),
    )?;

    // ── 2. Burn LP tokens from user (always) ─────────────────────────────────
    ctx.accounts.burn_lp(shares)?;

    // ── 3. Compute token amount and update market ─────────────────────────────
    let vault_balance = ctx.accounts.lend_vault.amount;
    let state_bump = ctx.bumps.state;
    let (immediate, lend_for_shares) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(pool.market)
            .with_irm(irm)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::InterestAccrualOverflow)?;
        let lend_for_shares = core
            .calc_lend_for_shares(shares)
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        // Shares worth nothing must not proceed down either path. The immediate
        // branch already excluded them, which left them falling through to the
        // queue — where a zero-amount entry costs the caller nothing, occupies
        // one of 1023 slots, and keeps the queue non-empty, which by itself
        // forces every subsequent withdrawal off the immediate path. Rejecting
        // here means the only way to take a slot is to have real value to claim.
        require!(
            lend_for_shares > 0,
            crate::error::ErrorCode::ZeroValueWithdrawal
        );
        // Pay out now whenever the vault can cover this exit *on top of* every
        // claim already queued — `borrowable_liquidity` is exactly that figure,
        // vault minus `assets_in_queue`, and it is the same reservation
        // `borrow` respects.
        //
        // The test used to be "is the queue empty", which made queue occupancy
        // rather than liquidity decide. One pending entry then forced every
        // later lender onto the queued path however liquid the pool was, so a
        // head that could not be paid held up exits that were fully funded, and
        // filling the 1023 slots stalled the lend side outright. Reserving the
        // queued assets gives queued lenders exactly what they are owed —
        // nothing behind them can spend it — while letting genuine surplus be
        // paid immediately. When the queue is empty `assets_in_queue` is 0 and
        // this reduces to the previous condition.
        let immediate =
            math::borrowable_liquidity(vault_balance, core.market.assets_in_queue)
                >= lend_for_shares;
        if immediate {
            core.withdraw_lent_immediate(shares, |amt| {
                ctx.accounts.transfer_lend_to_user(amt, state_bump)
            })
            .map_err(crate::error::ErrorCode::from)?;
        } else {
            // One authority must not be able to hold every slot — a full queue
            // blocks queueing for everyone, so slots are a shared resource even
            // though occupying one is nearly free.
            let requester = ctx.accounts.authority.key();
            require!(
                pool.withdrawal_queue
                    .count_for_capped(&requester, crate::withdrawal_queue::MAX_ENTRIES_PER_AUTHORITY)
                    < crate::withdrawal_queue::MAX_ENTRIES_PER_AUTHORITY,
                crate::error::ErrorCode::TooManyQueuedWithdrawals
            );
            core.withdraw_lent_queued(shares)
                .ok_or(crate::error::ErrorCode::MathOverflow)?;
            pool.withdrawal_queue
                .push(WithdrawalQueueEntry::new(requester, lend_for_shares))?;
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

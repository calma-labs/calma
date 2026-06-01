use crate::{
    error::ErrorCode,
    oracle::OracleState,
    state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition},
};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

#[derive(Accounts)]
#[instruction(amount: u64, duration: u64)]
pub struct BorrowWithHedge<'info> {
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

    /// The borrower.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The borrower's lend-token account (destination) — created if it doesn't exist yet.
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = lend_mint,
        associated_token::authority = authority,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault (source of borrowed tokens).
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The borrower's position — must have sufficient collateral.
    #[account(
        mut,
        seeds = [b"user_position", pool.key().as_ref(), authority.key().as_ref()],
        bump = user_position.load()?.bump,
        constraint = user_position.load()?.authority == authority.key() @ ErrorCode::Unauthorized,
        constraint = user_position.load()?.pool == pool.key() @ ErrorCode::InvalidAmount,
        constraint = user_position.load()?.collateral_deposited > 0 @ ErrorCode::InsufficientFunds,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    /// The rate-hedge offer being matched.
    ///
    /// Must belong to the same pool and have enough remaining capacity.
    #[account(
        mut,
        constraint = rate_hedge_offer.load()?.pool == pool.key() @ ErrorCode::InvalidAmount,
        constraint = rate_hedge_offer.load()?.amount >= amount @ ErrorCode::InsufficientFunds,
        constraint = duration >= rate_hedge_offer.load()?.min_duration @ ErrorCode::InvalidDurationRange,
        constraint = duration <= rate_hedge_offer.load()?.max_duration @ ErrorCode::InvalidDurationRange,
    )]
    pub rate_hedge_offer: AccountLoader<'info, RateHedgeOffer>,

    /// The match account created for this hedged borrow.
    ///
    /// Seeds: `["rate_hedge_match", user_position]` — one active match per position.
    #[account(
        init,
        payer = authority,
        space = 8 + std::mem::size_of::<RateHedgeMatch>(),
        seeds = [b"rate_hedge_match", user_position.key().as_ref()],
        bump,
    )]
    pub rate_hedge_match: AccountLoader<'info, RateHedgeMatch>,

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.irm_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    /// CHECK: feed state — price is read from its `value` field; key validated against pool.feed_state.
    #[account(constraint = feed_state.key() == pool.load()?.feed_state @ ErrorCode::InvalidAmount)]
    pub feed_state: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Borrow `amount` lend tokens and simultaneously lock in a fixed interest rate
/// via the provided `rate_hedge_offer`.
///
/// The upfront fee (`amount * fixed_rate_bps * duration / (10_000 * SECONDS_PER_YEAR)`)
/// is added to the borrower's debt shares on top of the borrow principal. No tokens
/// move for the fee at this point — `offer.locked_tokens` is credited as an accounting
/// entry; the physical transfer to the offer creator happens at settlement.
///
/// The match account stores the initial debt shares so the crank can later compare
/// actual variable growth against the borrower's fixed cap (`amount`).
impl<'info> BorrowWithHedge<'info> {
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

pub fn borrow_with_hedge_handler<'a>(
    ctx: Context<'a, BorrowWithHedge<'a>>,
    amount: u64,
    duration: u64,
) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(
        ctx.accounts.lend_vault.amount >= amount,
        ErrorCode::InsufficientFunds
    );

    // ── 1. Compute upfront fixed fee ──────────────────────────────────────────
    let fixed_rate_bps = ctx.accounts.rate_hedge_offer.load()?.fixed_rate_bps;
    let upfront_fee = jbl_math::compute_interest(amount, fixed_rate_bps as u32, duration)
        .ok_or(ErrorCode::MathOverflow)?;
    require!(upfront_fee > 0, ErrorCode::InvalidAmount);
    let total_debt_amount = amount.checked_add(upfront_fee).ok_or(ErrorCode::MathOverflow)?;

    // ── 2. Accrue interest + LTV check + share calculation ───────────────────
    let (oracle, utilization) = {
        let pool = ctx.accounts.pool.load()?;
        let oracle = OracleState::new(&ctx.accounts.sysvar_instructions.to_account_info(), pool.feed_program, pool.feed_state)?;
        (oracle, pool.calculate_utilization())
    };
    let current_ts = oracle.current_ts;
    require!(ctx.accounts.pool.load()?.lend_mint == ctx.accounts.lend_mint.key(), ErrorCode::InvalidMint);
    let oracle_price = crate::oracle::read_feed_price(&ctx.accounts.feed_state.to_account_info())?;
    let irm = crate::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    let state_bump = ctx.bumps.state;
    let new_shares = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = jbl_math::Core::new(pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?);
        core.accrue_interest().ok_or(ErrorCode::MathOverflow)?;
        let shares = core.borrow_with_fee(amount, total_debt_amount, oracle_price, |amt| ctx.accounts.transfer_lend_to_user(amt, state_bump))
            .map_err(|e| match e {
                jbl_math::MathError::Transfer(e) => e,
                e => ErrorCode::from(e).into(),
            })?;
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.debt_shares = core.position.debt_shares;
        shares
    };

    // ── 6. Update offer: reduce available capacity, credit locked tokens ──────
    {
        let mut offer = ctx.accounts.rate_hedge_offer.load_mut()?;
        offer.amount = offer
            .amount
            .checked_sub(amount)
            .ok_or(ErrorCode::MathOverflow)?;
        offer.locked_tokens = offer
            .locked_tokens
            .checked_add(upfront_fee)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    // ── 8. Initialise match account ───────────────────────────────────────────
    {
        let mut m = ctx.accounts.rate_hedge_match.load_init()?;
        m.offer = ctx.accounts.rate_hedge_offer.key();
        m.user_position = ctx.accounts.user_position.key();
        m.amount = amount;
        m.upfront_fee = upfront_fee;
        m.initial_debt_shares = new_shares;
        m.start_ts = current_ts;
        m.duration = duration;
        m.bump = ctx.bumps.rate_hedge_match;
    }

    msg!(
        "BorrowWithHedge: amount={} upfront_fee={} shares={} offer={} duration={}s",
        amount,
        upfront_fee,
        new_shares,
        ctx.accounts.rate_hedge_offer.key(),
        duration,
    );

    Ok(())
}

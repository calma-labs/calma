use crate::{
    constants::SECONDS_PER_YEAR,
    error::ErrorCode,
    math::{amount_to_shares, shares_to_amount},
    state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition},
};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

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
        space = 8 + 112, // RateHedgeMatch: 32+32+8+8+8+8+8+1+7 = 112
        seeds = [b"rate_hedge_match", user_position.key().as_ref()],
        bump,
    )]
    pub rate_hedge_match: AccountLoader<'info, RateHedgeMatch>,

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
pub fn borrow_with_hedge_handler(
    ctx: Context<BorrowWithHedge>,
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
    let upfront_fee: u64 = (amount as u128)
        .checked_mul(fixed_rate_bps as u128)
        .ok_or(ErrorCode::MathOverflow)?
        .checked_mul(duration as u128)
        .ok_or(ErrorCode::MathOverflow)?
        .checked_div(10_000u128.checked_mul(SECONDS_PER_YEAR as u128).ok_or(ErrorCode::MathOverflow)?)
        .ok_or(ErrorCode::MathOverflow)? as u64;
    require!(upfront_fee > 0, ErrorCode::InvalidAmount);

    let total_debt_amount = amount
        .checked_add(upfront_fee)
        .ok_or(ErrorCode::MathOverflow)?;

    // ── 2. Accrue interest ────────────────────────────────────────────────────
    let current_ts = Clock::get()?.unix_timestamp;
    ctx.accounts.pool.load_mut()?.accrue_interest(current_ts, ctx.remaining_accounts)?;

    // ── 3. LTV check and share calculation (covers amount + fee as new debt) ──
    let new_shares = {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.lend_mint.key() == pool.lend_mint,
            ErrorCode::InvalidMint
        );

        let collateral = ctx.accounts.user_position.load()?.collateral_deposited;
        let max_borrowable = collateral
            .checked_mul(pool.ltv_percent as u64)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::MathOverflow)?;

        let current_debt = if pool.market.total_borrow_shares > 0 {
            shares_to_amount(
                ctx.accounts.user_position.load()?.debt_shares,
                pool.market.total_borrow_assets,
                pool.market.total_borrow_shares,
            )
            .ok_or(ErrorCode::MathOverflow)?
        } else {
            0
        };

        let available = max_borrowable
            .checked_sub(current_debt)
            .ok_or(ErrorCode::InsufficientFunds)?;
        // LTV check uses only `amount` (principal); fee is offer creator's risk.
        require!(amount <= available, ErrorCode::InsufficientFunds);

        let shares = amount_to_shares(total_debt_amount, pool.market.total_borrow_assets, pool.market.total_borrow_shares)
            .ok_or(ErrorCode::MathOverflow)?;
        require!(shares > 0, ErrorCode::InvalidAmount);
        shares
    };

    // ── 4. Update user position ───────────────────────────────────────────────
    {
        let mut position = ctx.accounts.user_position.load_mut()?;
        position.debt_shares = position
            .debt_shares
            .checked_add(new_shares)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    // ── 5. Transfer `amount` lend tokens to the borrower (fee stays in pool) ──
    let seeds = &[b"state" as &[u8], &[ctx.bumps.state]];
    let signer = &[&seeds[..]];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.lend_vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.state.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;

    // ── 6. Update pool state ──────────────────────────────────────────────────
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_borrow_shares = pool
            .market
            .total_borrow_shares
            .checked_add(new_shares)
            .ok_or(ErrorCode::MathOverflow)?;
        pool.market.total_borrow_assets = pool
            .market
            .total_borrow_assets
            .checked_add(total_debt_amount)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    // ── 7. Update offer: reduce available capacity, credit locked tokens ──────
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

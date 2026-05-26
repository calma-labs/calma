use crate::{
    error::ErrorCode,
    math::{amount_to_shares, shares_to_amount},
    state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct SettleRateHedgeMatch<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs lend-vault-transfer CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint.
    #[account(
        constraint = lend_mint.key() == pool.load()?.lend_mint @ ErrorCode::InvalidMint
    )]
    pub lend_mint: Box<Account<'info, Mint>>,

    /// The match being settled. Closed after settlement; rent returned to cranker.
    #[account(
        mut,
        close = cranker,
        seeds = [b"rate_hedge_match", user_position.key().as_ref()],
        bump = rate_hedge_match.load()?.bump,
        constraint = rate_hedge_match.load()?.user_position == user_position.key() @ ErrorCode::InvalidAmount,
        constraint = rate_hedge_match.load()?.offer == rate_hedge_offer.key() @ ErrorCode::InvalidAmount,
    )]
    pub rate_hedge_match: AccountLoader<'info, RateHedgeMatch>,

    /// The offer that backed this match.
    #[account(
        mut,
        constraint = rate_hedge_offer.load()?.pool == pool.key() @ ErrorCode::InvalidAmount,
    )]
    pub rate_hedge_offer: AccountLoader<'info, RateHedgeOffer>,

    /// The borrower's position.
    #[account(
        mut,
        constraint = user_position.load()?.pool == pool.key() @ ErrorCode::InvalidAmount,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    /// The pool's lend vault — source of the upfront-fee payout to the offer creator.
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub lend_vault: Box<Account<'info, TokenAccount>>,

    /// The offer creator's lend-token account — receives the upfront fee.
    #[account(
        mut,
        constraint = offer_creator_token_account.owner == rate_hedge_offer.load()?.authority @ ErrorCode::Unauthorized,
        constraint = offer_creator_token_account.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub offer_creator_token_account: Box<Account<'info, TokenAccount>>,

    /// The offer creator's collateral vault — may be debited if the variable rate
    /// exceeded the fixed cap.
    #[account(
        mut,
        seeds = [b"rate_hedge_offer_vault", rate_hedge_offer.key().as_ref()],
        bump,
    )]
    pub offer_collateral_vault: Box<Account<'info, TokenAccount>>,

    /// The pool's collateral vault — receives any excess-variable-rate repayment
    /// from the offer creator's collateral.
    ///
    /// NOTE: The excess is denominated in collateral tokens (what the offer creator
    /// staked). Lend-side accounting is handled by adjusting debt shares.
    #[account(
        mut,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump,
    )]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,

    /// Anyone can crank a settlement once the duration has elapsed.
    #[account(mut)]
    pub cranker: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Settle a rate-hedge match after its duration has elapsed.
///
/// # Settlement mechanics
///
/// 1. Compute `current_value = shares_to_amount(initial_debt_shares)` — what the
///    debt (principal + fee + variable interest) has grown to.
/// 2. `excess = max(0, current_value − amount − upfront_fee)` — the variable interest
///    accrued beyond the fixed total that the offer creator owes.
/// 3. If `excess > 0`: transfer `excess` worth of collateral tokens from the offer
///    creator's vault to the pool's collateral vault (for now treated as simple
///    book-keeping; a full liquidation path is a later concern).
/// 4. Transfer `upfront_fee` lend tokens from the lend vault to the offer creator.
/// 5. Adjust the borrower's debt shares so their outstanding debt = `amount` (their cap).
/// 6. Restore offer capacity and decrement `locked_tokens`.
/// 7. Close the match account (rent returned to cranker).
pub fn settle_rate_hedge_match_handler(ctx: Context<SettleRateHedgeMatch>) -> Result<()> {
    let current_ts = Clock::get()?.unix_timestamp;

    // ── 0. Duration guard ─────────────────────────────────────────────────────
    let (settlement_ts, initial_debt_shares, borrow_amount, upfront_fee) = {
        let m = ctx.accounts.rate_hedge_match.load()?;
        let settlement_ts = m
            .start_ts
            .checked_add(m.duration as i64)
            .ok_or(ErrorCode::MathOverflow)?;
        (settlement_ts, m.initial_debt_shares, m.amount, m.upfront_fee)
    };
    require!(current_ts >= settlement_ts, ErrorCode::HedgeNotYetMatured);

    // ── 1. Accrue interest so shares reflect current state ────────────────────
    ctx.accounts.pool.load_mut()?.accrue_interest(current_ts, ctx.remaining_accounts)?;

    // ── 2. Snapshot match fields already done above ───────────────────────────
    let fixed_total = borrow_amount
        .checked_add(upfront_fee)
        .ok_or(ErrorCode::MathOverflow)?;

    // ── 3. Compute current variable value of the initial debt shares ──────────
    let current_value = {
        let pool = ctx.accounts.pool.load()?;
        shares_to_amount(initial_debt_shares, pool.market.total_borrow_assets, pool.market.total_borrow_shares)
            .ok_or(ErrorCode::MathOverflow)?
    };

    // ── 4. Compute excess owed by offer creator ───────────────────────────────
    let excess = current_value.saturating_sub(fixed_total);

    let state_bump = ctx.bumps.state;
    let state_seeds: &[&[u8]] = &[b"state", &[state_bump]];
    let signer = &[state_seeds];

    // ── 5. If excess > 0 — transfer collateral from offer vault to pool vault ─
    //
    // The collateral is denominated in the pool's collateral token. The `excess`
    // is a lend-token amount; for now we treat it as a 1:1 token count transfer
    // (oracle-based conversion is a later concern).
    if excess > 0 {
        let available = ctx.accounts.offer_collateral_vault.amount;
        let transfer_amount = excess.min(available); // never panic if under-collateralised
        if transfer_amount > 0 {
            anchor_spl::token::transfer(
                CpiContext::new_with_signer(
                    *ctx.accounts.token_program.to_account_info().key,
                    anchor_spl::token::Transfer {
                        from: ctx.accounts.offer_collateral_vault.to_account_info(),
                        to: ctx.accounts.collateral_vault.to_account_info(),
                        authority: ctx.accounts.state.to_account_info(),
                    },
                    signer,
                ),
                transfer_amount,
            )?;
        }
    }

    // ── 6. Transfer upfront fee from lend vault to offer creator ─────────────
    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.lend_vault.to_account_info(),
                to: ctx.accounts.offer_creator_token_account.to_account_info(),
                authority: ctx.accounts.state.to_account_info(),
            },
            signer,
        ),
        upfront_fee,
    )?;

    // ── 7. Adjust borrower's debt shares to cap at `borrow_amount` ───────────
    //
    // Remove the initial_debt_shares from the user's position and from pool totals,
    // then re-issue shares worth `borrow_amount` at the current pool ratio.
    {
        let mut pool = ctx.accounts.pool.load_mut()?;

        // Remove old shares from pool.
        pool.market.total_borrow_shares = pool
            .market
            .total_borrow_shares
            .checked_sub(initial_debt_shares)
            .ok_or(ErrorCode::MathOverflow)?;
        pool.market.total_borrow_assets = pool
            .market
            .total_borrow_assets
            .checked_sub(current_value)
            .ok_or(ErrorCode::MathOverflow)?;

        // Account for the upfront fee that just left the pool.
        pool.market.total_supply_assets = pool
            .market
            .total_supply_assets
            .saturating_sub(upfront_fee);

        // Re-issue shares for the borrower's capped amount.
        let new_shares =
            amount_to_shares(borrow_amount, pool.market.total_borrow_assets, pool.market.total_borrow_shares)
                .ok_or(ErrorCode::MathOverflow)?;

        pool.market.total_borrow_shares = pool
            .market
            .total_borrow_shares
            .checked_add(new_shares)
            .ok_or(ErrorCode::MathOverflow)?;
        pool.market.total_borrow_assets = pool
            .market
            .total_borrow_assets
            .checked_add(borrow_amount)
            .ok_or(ErrorCode::MathOverflow)?;

        // Update user position.
        let new_debt_shares = ctx.accounts.user_position.load()?.debt_shares
            .checked_sub(initial_debt_shares)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_add(new_shares)
            .ok_or(ErrorCode::MathOverflow)?;
        ctx.accounts.user_position.load_mut()?.debt_shares = new_debt_shares;
    }

    // ── 8. Update offer ───────────────────────────────────────────────────────
    {
        let mut offer = ctx.accounts.rate_hedge_offer.load_mut()?;
        offer.amount = offer
            .amount
            .checked_add(borrow_amount)
            .ok_or(ErrorCode::MathOverflow)?;
        offer.locked_tokens = offer
            .locked_tokens
            .checked_sub(upfront_fee)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    msg!(
        "SettleRateHedgeMatch: current_value={} fixed_total={} excess={} upfront_fee={}",
        current_value,
        fixed_total,
        excess,
        upfront_fee,
    );

    // ── 9. Close match account — rent returned to cranker ────────────────────
    // Anchor handles account closing via the `close` constraint. We use a manual
    // lamport drain here since close= is set in the struct attribute below.
    // (See struct attribute: `close = cranker` added via Anchor's #[account(close=...)].)

    Ok(())
}

use crate::{
    error::ErrorCode,
    oracle::OracleState,
    state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

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

    /// CHECK: validated as pool.rate_program
    #[account(constraint = rate_program.key() == pool.load()?.rate_program @ ErrorCode::MissingRateProgram)]
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: validated as pool.rate_state
    #[account(constraint = irm_state.key() == pool.load()?.irm_state @ ErrorCode::MissingRateState)]
    pub irm_state: UncheckedAccount<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

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
impl<'info> SettleRateHedgeMatch<'info> {
    pub fn transfer_excess_collateral_to_pool(&self, amount: u64, state_bump: u8) -> Result<()> {
        let state_seeds: &[&[u8]] = &[b"state", &[state_bump]];
        let signer = &[state_seeds];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.offer_collateral_vault.to_account_info(),
                    to: self.collateral_vault.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }

    pub fn transfer_upfront_fee_to_offer_creator(&self, amount: u64, state_bump: u8) -> Result<()> {
        let state_seeds: &[&[u8]] = &[b"state", &[state_bump]];
        let signer = &[state_seeds];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.lend_vault.to_account_info(),
                    to: self.offer_creator_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn settle_rate_hedge_match_handler<'a>(ctx: Context<'a, SettleRateHedgeMatch<'a>>) -> Result<()> {
    let (oracle, utilization) = {
        let pool = ctx.accounts.pool.load()?;
        let oracle = OracleState::new(&ctx.accounts.sysvar_instructions.to_account_info(), pool.feed_program, pool.feed_state)?;
        (oracle, pool.calculate_utilization())
    };
    let current_ts = oracle.current_ts;

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

    // ── 1. Accrue interest, settle shares, update pool + position ────────────
    let irm = crate::irm::IrmState::new(ctx.accounts.rate_program.to_account_info(), utilization, ctx.accounts.pool.to_account_info(), ctx.accounts.irm_state.to_account_info())?;
    let state_bump = ctx.bumps.state;
    let current_value = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = jbl_math::Core::new(pool.market)
            .with_oracle(oracle)
            .with_irm(irm)
            .with_position(*ctx.accounts.user_position.load()?);
        core.accrue_interest().ok_or(ErrorCode::MathOverflow)?;
        let (current_value, _new_shares) = core
            .settle_hedge(
                initial_debt_shares,
                borrow_amount,
                upfront_fee,
                |excess| {
                    let available = ctx.accounts.offer_collateral_vault.amount;
                    let transfer_amount = excess.min(available);
                    if transfer_amount > 0 {
                        ctx.accounts.transfer_excess_collateral_to_pool(transfer_amount, state_bump)
                    } else {
                        Ok(())
                    }
                },
                |fee| ctx.accounts.transfer_upfront_fee_to_offer_creator(fee, state_bump),
            )
            .map_err(crate::error::ErrorCode::from)?;
        pool.market = core.market;
        ctx.accounts.user_position.load_mut()?.debt_shares = core.position.debt_shares;
        current_value
    };

    let fixed_total = borrow_amount
        .checked_add(upfront_fee)
        .ok_or(ErrorCode::MathOverflow)?;
    let excess = current_value.saturating_sub(fixed_total);

    // ── 4. Update offer ───────────────────────────────────────────────────────
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

use crate::{
    error::ErrorCode,
    state::{Pool, RateHedgeOffer},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
#[instruction(fixed_rate_bps: u64, min_duration: u64, max_duration: u64, amount: u64, collateral_amount: u64)]
pub struct CreateRateHedgeOffer<'info> {
    /// The lending pool this offer is associated with.
    pub pool: AccountLoader<'info, Pool>,

    /// The offer PDA.
    ///
    /// Seeds encode the offer parameters so two offers from the same user with different
    /// rates/durations occupy separate accounts and cannot alias.
    #[account(
        init,
        payer = authority,
        space = 8 + std::mem::size_of::<RateHedgeOffer>(),
        seeds = [
            b"rate_hedge_offer",
            pool.key().as_ref(),
            authority.key().as_ref(),
            &fixed_rate_bps.to_le_bytes(),
            &min_duration.to_le_bytes(),
            &max_duration.to_le_bytes(),
        ],
        bump,
    )]
    pub rate_hedge_offer: AccountLoader<'info, RateHedgeOffer>,

    /// Token vault that holds the collateral locked for this offer.
    ///
    /// Authority is the program-wide `state` PDA so only this program can move tokens out.
    #[account(
        init,
        payer = authority,
        token::mint = collateral_mint,
        token::authority = state,
        seeds = [b"rate_hedge_offer_vault", rate_hedge_offer.key().as_ref()],
        bump,
    )]
    pub offer_collateral_vault: Account<'info, TokenAccount>,

    /// CHECK: Signer-only PDA — no data stored; used as authority for all vault accounts.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The collateral mint for this pool.
    #[account(
        constraint = collateral_mint.key() == pool.load()?.collateral_mint @ ErrorCode::InvalidMint
    )]
    pub collateral_mint: Account<'info, Mint>,

    /// The user's collateral token account (source of the locked collateral).
    #[account(
        mut,
        constraint = user_collateral_token_account.owner == authority.key() @ ErrorCode::Unauthorized,
        constraint = user_collateral_token_account.mint == collateral_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub user_collateral_token_account: Account<'info, TokenAccount>,

    /// The user creating the offer. Pays for account rent and provides collateral.
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Creates a new rate-hedge offer and locks `collateral_amount` tokens as security.
///
/// # Parameters
/// - `fixed_rate_bps`   – The fixed interest rate offered, in basis points.
/// - `min_duration`     – Minimum acceptable match duration in seconds.
/// - `max_duration`     – Maximum acceptable match duration in seconds.
/// - `amount`           – Notional lend-token amount this offer covers.
/// - `collateral_amount`– Collateral tokens locked on creation.
pub fn create_rate_hedge_offer_handler(
    ctx: Context<CreateRateHedgeOffer>,
    fixed_rate_bps: u64,
    min_duration: u64,
    max_duration: u64,
    amount: u64,
    collateral_amount: u64,
) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(collateral_amount > 0, ErrorCode::InvalidAmount);
    require!(fixed_rate_bps > 0, ErrorCode::InvalidAmount);
    require!(
        min_duration > 0 && max_duration >= min_duration,
        ErrorCode::InvalidDurationRange
    );

    // Transfer collateral from the user into the offer vault.
    anchor_spl::token::transfer(
        CpiContext::new(
            *ctx.accounts.token_program.to_account_info().key,
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_collateral_token_account.to_account_info(),
                to: ctx.accounts.offer_collateral_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        collateral_amount,
    )?;

    // Initialise the offer account.
    {
        let mut offer = ctx.accounts.rate_hedge_offer.load_init()?;
        offer.pool = ctx.accounts.pool.key();
        offer.authority = ctx.accounts.authority.key();
        offer.amount = amount;
        offer.fixed_rate_bps = fixed_rate_bps;
        offer.min_duration = min_duration;
        offer.max_duration = max_duration;
        offer.collateral_deposited = collateral_amount;
        offer.bump = ctx.bumps.rate_hedge_offer;
    }

    msg!(
        "RateHedgeOffer created: pool={} authority={} amount={} rate_bps={} min_dur={} max_dur={} collateral={}",
        ctx.accounts.pool.key(),
        ctx.accounts.authority.key(),
        amount,
        fixed_rate_bps,
        min_duration,
        max_duration,
        collateral_amount,
    );

    Ok(())
}

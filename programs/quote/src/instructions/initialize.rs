use anchor_lang::prelude::*;
use anchor_spl::token::Mint;
use interface::PriceFeedHeader;

use crate::{error::ErrorCode, state::Provider, MAX_RATE_BPS};

/// Stand up a provider for `pool`, priced and rated in one instruction.
///
/// # The seeds are not ours to choose
///
/// `calma::create` derives `["irm_config", pool]` under whichever program a
/// market names as its rate program and requires the account it is handed to
/// equal that address. The seed literal is therefore part of the interface, not
/// a local convention — a provider that seeded its account differently could
/// serve prices but could never be a market's rate model.
///
/// # Permissionless, and first-come-first-served
///
/// Anyone may claim the provider account for a given pool, exactly as
/// `irm::initialize` may be. Whoever claims it names themselves `authority` for
/// the life of the market. That is safe only because `calma::create` asks the
/// provider, over the `check_authority` ABI method, whether the market's own
/// creator controls it — a market whose provider was claimed by someone else
/// cannot be created, rather than being created and quietly controlled by them.
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<Provider>(),
        seeds = [b"irm_config", pool.key().as_ref()],
        bump,
    )]
    pub provider: Account<'info, Provider>,

    /// CHECK: seed source only. At market-creation time this is still an
    /// unwritten, `calma`-owned buffer, so there is nothing here to deserialize.
    pub pool: UncheckedAccount<'info>,

    pub collateral_mint: Account<'info, Mint>,
    pub lend_mint: Account<'info, Mint>,

    /// The key that will be allowed to move this provider's price and rate.
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_handler(
    ctx: Context<Initialize>,
    price_ttl_ms: u32,
    flat_rate_bps: u32,
    collateral_price: u64,
    lend_price: u64,
) -> Result<()> {
    // `0` is the fail-closed sentinel every price consumer reads, so a provider
    // created with it could never be borrowed against.
    require!(price_ttl_ms > 0, ErrorCode::InvalidPriceTtl);
    require!(flat_rate_bps <= MAX_RATE_BPS, ErrorCode::RateTooHigh);
    require!(
        collateral_price > 0 && lend_price > 0,
        ErrorCode::ZeroPrice
    );

    let provider = &mut ctx.accounts.provider;
    provider.header = PriceFeedHeader {
        collateral_mint: ctx.accounts.collateral_mint.key(),
        lend_mint: ctx.accounts.lend_mint.key(),
        collateral_price,
        lend_price,
        collateral_decimals: ctx.accounts.collateral_mint.decimals,
        lend_decimals: ctx.accounts.lend_mint.decimals,
        last_updated_ts: Clock::get()?.unix_timestamp,
        price_ttl_ms,
    };
    provider.pool = ctx.accounts.pool.key();
    provider.authority = ctx.accounts.authority.key();
    provider.bump = ctx.bumps.provider;
    provider.flat_rate_bps = flat_rate_bps;
    Ok(())
}

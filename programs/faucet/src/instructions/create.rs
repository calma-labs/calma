use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

/// Upper bound on a mint symbol. Seeds are capped at 32 bytes anyway; keeping
/// symbols short also keeps the derived address easy to reproduce client-side.
pub const MAX_SYMBOL_LEN: usize = 8;

#[derive(Accounts)]
#[instruction(symbol: String, decimals: u8)]
pub struct Create<'info> {
    /// Pays the mint's rent. Anyone may create a faucet mint — this program is
    /// test-only and every mint it creates is worthless by construction.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Signer-only PDA — no data stored; holds mint authority over every faucet mint.
    #[account(
        seeds = [crate::MINT_AUTHORITY_SEED.as_bytes()],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// The mint being created. Derived from its symbol so clients can find it
    /// without keeping a table of mint addresses.
    #[account(
        init,
        payer = payer,
        seeds = [crate::MINT_SEED.as_bytes(), symbol.as_bytes()],
        bump,
        mint::decimals = decimals,
        mint::authority = mint_authority,
    )]
    pub mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Create a faucet mint under the program's `mint_authority` PDA.
///
/// The PDA — not the caller — ends up as the mint authority, which is what makes
/// `mint` permissionless: nobody has to hold or ship a private key to hand out
/// test tokens. There is no freeze authority.
pub fn create_handler(ctx: Context<Create>, symbol: String, decimals: u8) -> Result<()> {
    require!(
        !symbol.is_empty() && symbol.len() <= MAX_SYMBOL_LEN,
        ErrorCode::InvalidSymbol
    );

    msg!(
        "Faucet: created mint {} ({}, {} decimals) under authority {}",
        ctx.accounts.mint.key(),
        symbol,
        decimals,
        ctx.accounts.mint_authority.key(),
    );

    Ok(())
}

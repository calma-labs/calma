use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct MockSwap<'info> {
    /// CHECK: Signer-only PDA — no data stored; holds mint authority over both mints
    /// and signs the mint_to CPI.
    #[account(
        seeds = [crate::MINT_AUTHORITY_SEED.as_bytes()],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// The owner of the token accounts — must sign to authorize burning from their account.
    pub token_owner: Signer<'info>,

    /// The mint to burn from. Must be under the faucet's authority — burning needs no
    /// authority at all, but the constraint keeps the swap confined to test mints.
    #[account(
        mut,
        constraint = mint_in.mint_authority == COption::Some(mint_authority.key()) @ ErrorCode::Unauthorized,
    )]
    pub mint_in: Account<'info, Mint>,

    /// The mint to issue tokens from. Must be under the faucet's authority.
    #[account(
        mut,
        constraint = mint_out.mint_authority == COption::Some(mint_authority.key()) @ ErrorCode::Unauthorized,
    )]
    pub mint_out: Account<'info, Mint>,

    /// The caller's token account to burn from.
    #[account(
        mut,
        constraint = user_token_in.owner == token_owner.key() @ ErrorCode::Unauthorized,
        constraint = user_token_in.mint == mint_in.key() @ ErrorCode::InvalidMint,
    )]
    pub user_token_in: Account<'info, TokenAccount>,

    /// The caller's token account to receive minted tokens.
    #[account(
        mut,
        constraint = user_token_out.owner == token_owner.key() @ ErrorCode::Unauthorized,
        constraint = user_token_out.mint == mint_out.key() @ ErrorCode::InvalidMint,
    )]
    pub user_token_out: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

/// Mock 1:1 swap: burn `amount` of `mint_in` from the caller, mint `amount` of `mint_out`
/// to the caller. Both mints must be under the faucet's `mint_authority` PDA, which
/// signs the mint side — so no caller holds a key that can issue these tokens.
///
/// This instruction exists solely for testing and local-validator faucet scenarios.
/// It must never be deployed to mainnet.
impl<'info> MockSwap<'info> {
    pub fn burn_token_in(&self, amount: u64) -> Result<()> {
        anchor_spl::token::burn(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Burn {
                    mint: self.mint_in.to_account_info(),
                    from: self.user_token_in.to_account_info(),
                    authority: self.token_owner.to_account_info(),
                },
            ),
            amount,
        )
    }

    pub fn mint_token_out(&self, amount: u64, authority_bump: u8) -> Result<()> {
        let seeds = &[crate::MINT_AUTHORITY_SEED.as_bytes(), &[authority_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::MintTo {
                    mint: self.mint_out.to_account_info(),
                    to: self.user_token_out.to_account_info(),
                    authority: self.mint_authority.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn mock_swap_handler(ctx: Context<MockSwap>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);

    // Burn `amount` of mint_in from the caller's account.
    ctx.accounts.burn_token_in(amount)?;

    // Mint `amount` of mint_out to the caller's account.
    ctx.accounts
        .mint_token_out(amount, ctx.bumps.mint_authority)?;

    msg!(
        "MockSwap: burned {} of {}, minted {} of {}",
        amount,
        ctx.accounts.mint_in.key(),
        amount,
        ctx.accounts.mint_out.key(),
    );

    Ok(())
}

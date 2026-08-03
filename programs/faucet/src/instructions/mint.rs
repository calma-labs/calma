use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct MintTokens<'info> {
    /// Pays for the recipient's token account when it has to be created.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: only used as the owner of `recipient_token_account`; the associated-token
    /// constraint below is what binds the two together.
    pub recipient: UncheckedAccount<'info>,

    /// CHECK: Signer-only PDA — no data stored; signs the mint_to CPI.
    #[account(
        seeds = [crate::MINT_AUTHORITY_SEED.as_bytes()],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// The mint to issue from. Must already be under the faucet's authority,
    /// which is true of every mint `create` produced.
    #[account(
        mut,
        constraint = mint.mint_authority == COption::Some(mint_authority.key()) @ ErrorCode::Unauthorized,
    )]
    pub mint: Account<'info, Mint>,

    /// The recipient's associated token account — created if it doesn't exist.
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub recipient_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> MintTokens<'info> {
    pub fn mint_to_recipient(&self, amount: u64, authority_bump: u8) -> Result<()> {
        let seeds = &[crate::MINT_AUTHORITY_SEED.as_bytes(), &[authority_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::MintTo {
                    mint: self.mint.to_account_info(),
                    to: self.recipient_token_account.to_account_info(),
                    authority: self.mint_authority.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

/// Mint `amount` of a faucet-controlled mint to `recipient`.
///
/// Permissionless on purpose: the mint authority is a program PDA, so any caller
/// can hand test tokens to any address without a shipped keypair. The amount is
/// uncapped — these mints only ever exist on local validators and devnet.
pub fn mint_handler(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);

    ctx.accounts
        .mint_to_recipient(amount, ctx.bumps.mint_authority)?;

    msg!(
        "Faucet: minted {} of {} to {}",
        amount,
        ctx.accounts.mint.key(),
        ctx.accounts.recipient.key(),
    );

    Ok(())
}

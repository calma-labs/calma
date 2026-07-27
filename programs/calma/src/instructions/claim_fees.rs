use crate::state::Pool;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, MintTo, Token, TokenAccount};

#[derive(Accounts)]
pub struct ClaimFees<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs the LP-mint CPI.
    #[account(seeds = [b"state"], bump)]
    pub state: UncheckedAccount<'info>,

    /// The LP token mint for this lending pool.
    #[account(
        mut,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump,
    )]
    pub lp_mint: Account<'info, Mint>,

    /// Must be the pool authority; receives the accrued fee as LP tokens.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The authority's LP token account (destination for claimed fee shares).
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = lp_mint,
        associated_token::authority = authority,
    )]
    pub authority_lp_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> ClaimFees<'info> {
    fn mint_lp_to_authority(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                MintTo {
                    mint: self.lp_mint.to_account_info(),
                    to: self.authority_lp_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

/// Claim the protocol's accrued fee. The fee was minted into
/// `total_supply_shares` at accrual but never issued as LP tokens; this mints
/// the matching LP tokens to the pool authority and resets the counter, so LP
/// supply reconciles with `total_supply_shares`. The authority can then redeem
/// them through the normal `withdraw_lent` path.
pub fn claim_fees_handler(ctx: Context<ClaimFees>) -> Result<()> {
    let state_bump = ctx.bumps.state;
    let fee_shares = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require!(
            pool.authority == ctx.accounts.authority.key(),
            crate::error::ErrorCode::Unauthorized
        );
        let shares = pool.market.accrued_fee_shares;
        require!(shares > 0, crate::error::ErrorCode::NoFeesToClaim);
        // total_supply_shares already includes these; only the counter resets.
        pool.market.accrued_fee_shares = 0;
        shares
    };
    ctx.accounts.mint_lp_to_authority(fee_shares, state_bump)?;
    msg!("ClaimFees: minted {} LP tokens to authority", fee_shares);
    Ok(())
}

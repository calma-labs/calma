use crate::{error::ErrorCode, state::Pool};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

use super::flash_borrow::FLASH_BORROW_DISCRIMINATOR;

#[derive(Accounts)]
pub struct FlashRepay<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The lend token mint.
    #[account(
        constraint = lend_mint.key() == pool.load()?.lend_mint @ ErrorCode::InvalidMint
    )]
    pub lend_mint: Account<'info, Mint>,

    /// The pool's lend vault — receives the repayment.
    #[account(
        mut,
        seeds = [::state::seeds::LEND_VAULT, pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The repayer's lend-token account — source of the repayment.
    #[account(
        mut,
        constraint = user_source.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub user_source: Account<'info, TokenAccount>,

    /// The repayer (must be a signer so only the flash-loan recipient can repay).
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

impl<'info> FlashRepay<'info> {
    pub fn transfer_repayment_to_vault(&self, amount: u64) -> Result<()> {
        anchor_spl::token::transfer(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.user_source.to_account_info(),
                    to: self.lend_vault.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
        )
    }
}

pub fn flash_repay_handler(ctx: Context<FlashRepay>, amount: u64) -> Result<()> {
    // ── 1. Verify a matching flash_borrow preceded this instruction ───────────
    //
    // Chain-shaped check (instruction sysvar), retained as defence in depth: it
    // pins the repay to a borrow against *this* pool earlier in *this*
    // transaction. The amount/pairing rules live in `Core::flash_repay`.
    let sysvar_info = ctx.accounts.sysvar_instructions.to_account_info();
    let current_index = load_current_index_checked(&sysvar_info)? as usize;
    let pool_key = ctx.accounts.pool.key();

    let mut saw_borrow = false;
    for idx in 0..current_index {
        let ix = match load_instruction_at_checked(idx, &sysvar_info) {
            Ok(ix) => ix,
            Err(_) => continue,
        };

        if ix.program_id == crate::ID
            && ix.data.len() >= 16
            && ix.data[..8] == FLASH_BORROW_DISCRIMINATOR
            && ix
                .accounts
                .first()
                .map(|a| a.pubkey == pool_key)
                .unwrap_or(false)
        {
            saw_borrow = true;
            break;
        }
    }
    require!(saw_borrow, ErrorCode::FlashBorrowMissing);

    // ── 2. Close the loan via Core, which performs the token transfer ─────────
    let user_balance = ctx.accounts.user_source.amount;
    let (principal, fee) = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(&mut pool.market);
        let result = core
            .flash_repay(amount, |amt| {
                require!(user_balance >= amt, ErrorCode::InsufficientFunds);
                ctx.accounts.transfer_repayment_to_vault(amt)
            })
            .map_err(|e| match e {
                math::MathError::Transfer(e) => e,
                e => ErrorCode::from(e).into(),
            })?;
        result
    };

    msg!(
        "FlashRepay: principal={} fee={} repaid={}",
        principal,
        fee,
        amount,
    );

    Ok(())
}

use crate::{error::ErrorCode, state::Pool};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

use super::flash_borrow::FLASH_BORROW_DISCRIMINATOR;
use crate::FLASH_LOAN_FEE_BPS;

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
        seeds = [b"lend_vault", pool.key().as_ref()],
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
    require!(amount > 0, ErrorCode::InvalidAmount);

    // ── 1. Verify a matching flash_borrow preceded this instruction ───────────
    //
    // We scan every instruction that comes *before* the current one in the
    // transaction. We require at least one that:
    //   a) targets this program
    //   b) has the flash_borrow discriminator
    //   c) references the same pool (first account)
    //
    // We also extract the borrowed amount from the flash_borrow instruction data
    // (bytes [8..16]) to verify that `amount >= borrowed + fee`.
    let sysvar_info = ctx.accounts.sysvar_instructions.to_account_info();
    let current_index = load_current_index_checked(&sysvar_info)? as usize;

    let pool_key = ctx.accounts.pool.key();

    let mut borrowed_amount: Option<u64> = None;
    for idx in 0..current_index {
        let ix = match load_instruction_at_checked(idx, &sysvar_info) {
            Ok(ix) => ix,
            Err(_) => continue,
        };

        if ix.program_id == crate::ID
            && ix.data.len() >= 16
            && ix.data[..8] == FLASH_BORROW_DISCRIMINATOR
        {
            let pool_matches = ix
                .accounts
                .first()
                .map(|a| a.pubkey == pool_key)
                .unwrap_or(false);

            if pool_matches {
                borrowed_amount = Some(u64::from_le_bytes(ix.data[8..16].try_into().unwrap()));
                break;
            }
        }
    }

    let borrowed = borrowed_amount.ok_or(ErrorCode::FlashBorrowMissing)?;
    let fee =
        math::flash_fee(borrowed, FLASH_LOAN_FEE_BPS as u32).ok_or(ErrorCode::MathOverflow)?;
    let min_repay = borrowed.checked_add(fee).ok_or(ErrorCode::MathOverflow)?;
    require!(amount >= min_repay, ErrorCode::FlashLoanFeeNotCovered);

    // ── 2. Verify repayer has sufficient balance ──────────────────────────────
    require!(
        ctx.accounts.user_source.amount >= amount,
        ErrorCode::InsufficientFunds
    );

    // ── 3. Transfer repayment from user to lend vault ─────────────────────────
    ctx.accounts.transfer_repayment_to_vault(amount)?;

    // ── 4. Update pool accounting ─────────────────────────────────────────────
    //
    // The full repay amount (principal + fee) is credited back to the pool.
    // The net effect vs the flash_borrow is +fee for depositors.
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_supply_assets = pool
            .market
            .total_supply_assets
            .checked_add(amount)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    msg!(
        "FlashRepay: borrowed={} fee={} repaid={}",
        borrowed,
        fee,
        amount,
    );

    Ok(())
}

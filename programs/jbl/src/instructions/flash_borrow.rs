use crate::{error::ErrorCode, state::Pool, FLASH_LOAN_FEE_BPS};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

/// Anchor discriminator for `flash_repay` = sha256("global:flash_repay")[0..8].
/// Pre-computed: python3 -c "import hashlib; print(list(hashlib.sha256(b'global:flash_repay').digest()[:8]))"
pub const FLASH_REPAY_DISCRIMINATOR: [u8; 8] = [182, 143, 19, 23, 39, 221, 184, 78];

/// Anchor discriminator for `flash_borrow` = sha256("global:flash_borrow")[0..8].
pub const FLASH_BORROW_DISCRIMINATOR: [u8; 8] = [166, 221, 220, 25, 61, 73, 127, 240];


#[derive(Accounts)]
pub struct FlashBorrow<'info> {
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
    pub lend_mint: Account<'info, Mint>,

    /// The pool's lend vault — source of flash-loaned tokens.
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The receiver's lend-token account — destination of the flash loan.
    #[account(
        mut,
        constraint = user_destination.mint == lend_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub user_destination: Account<'info, TokenAccount>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

impl<'info> FlashBorrow<'info> {
    pub fn transfer_lend_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let state_seeds: &[&[u8]] = &[b"state", &[state_bump]];
        let signer = &[state_seeds];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.lend_vault.to_account_info(),
                    to: self.user_destination.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn flash_borrow_handler(ctx: Context<FlashBorrow>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(
        ctx.accounts.lend_vault.amount >= amount,
        ErrorCode::InsufficientFunds
    );

    // ── 1. Verify a matching flash_repay follows in this transaction ──────────
    //
    // We scan every instruction that comes *after* the current one in the
    // transaction. We require at least one that:
    //   a) targets this program
    //   b) has the flash_repay discriminator
    //   c) references the same pool (first account)
    //   d) carries a repay amount >= amount + fee (bytes [8..16])
    let sysvar_info = ctx.accounts.sysvar_instructions.to_account_info();
    let current_index = load_current_index_checked(&sysvar_info)? as usize;

    let fee = jbl_math::flash_fee(amount, FLASH_LOAN_FEE_BPS as u32).ok_or(ErrorCode::MathOverflow)?;
    let min_repay = amount.checked_add(fee).ok_or(ErrorCode::MathOverflow)?;
    let pool_key = ctx.accounts.pool.key();

    let mut found = false;
    let mut idx = current_index + 1;
    loop {
        let ix = match load_instruction_at_checked(idx, &sysvar_info) {
            Ok(ix) => ix,
            Err(_) => break,
        };

        if ix.program_id == crate::ID
            && ix.data.len() >= 16
            && ix.data[..8] == FLASH_REPAY_DISCRIMINATOR
        {
            // Account[0] of flash_repay is the pool.
            let repay_amount = u64::from_le_bytes(ix.data[8..16].try_into().unwrap());
            let pool_matches = ix
                .accounts
                .first()
                .map(|a| a.pubkey == pool_key)
                .unwrap_or(false);

            if pool_matches && repay_amount >= min_repay {
                found = true;
                break;
            }
        }
        idx += 1;
    }
    require!(found, ErrorCode::FlashRepayMissing);

    // ── 2. Transfer tokens from lend vault to user ────────────────────────────
    ctx.accounts.transfer_lend_to_user(amount, ctx.bumps.state)?;

    // ── 3. Update pool accounting ─────────────────────────────────────────────
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.market.total_supply_assets = pool
            .market
            .total_supply_assets
            .checked_sub(amount)
            .ok_or(ErrorCode::MathOverflow)?;
    }

    msg!(
        "FlashBorrow: amount={} fee={} min_repay={}",
        amount,
        fee,
        min_repay,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{FLASH_BORROW_DISCRIMINATOR, FLASH_REPAY_DISCRIMINATOR};
    use sha2::{Digest, Sha256};

    fn anchor_discriminator(ix_name: &str) -> [u8; 8] {
        let preimage = format!("global:{ix_name}");
        let hash = Sha256::digest(preimage.as_bytes());
        hash[..8].try_into().unwrap()
    }

    #[test]
    fn flash_repay_discriminator_is_correct() {
        assert_eq!(
            FLASH_REPAY_DISCRIMINATOR,
            anchor_discriminator("flash_repay"),
        );
    }

    #[test]
    fn flash_borrow_discriminator_is_correct() {
        assert_eq!(
            FLASH_BORROW_DISCRIMINATOR,
            anchor_discriminator("flash_borrow"),
        );
    }
}

use crate::{error::ErrorCode, state::Pool};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
use solana_sdk_ids::sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID;

#[constant]
pub const POOL_SPACE: u64 = (8 + std::mem::size_of::<Pool>()) as u64;

/// Anchor discriminator for `set_value` = sha256("global:set_value")[0..8].
/// Pre-computed: python3 -c "import hashlib; print(list(hashlib.sha256(b'global:set_value').digest()[:8]))"
pub const SET_VALUE_DISCRIMINATOR: [u8; 8] = [253, 214, 48, 201, 100, 201, 227, 219];

#[derive(Accounts)]
pub struct Create<'info> {
    /// The pool data account.  Must be pre-allocated (size = POOL_SPACE) and
    /// owned by this program before calling `create`.  Pre-allocating in a
    /// separate transaction bypasses the 10 KB CPI account-creation limit.
    #[account(zero)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; used as authority for vault token accounts and LP mint.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// Token vault for holding deposited collateral tokens.
    #[account(
        init,
        payer = payer,
        token::mint = collateral_mint,
        token::authority = state,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// Token vault for holding deposited lend tokens.
    #[account(
        init,
        payer = payer,
        token::mint = lend_mint,
        token::authority = state,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The LP token mint for the lend side of this pool.
    #[account(
        init,
        payer = payer,
        mint::decimals = lend_mint.decimals,
        mint::authority = state,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump
    )]
    pub lp_mint: Account<'info, Mint>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The lend token mint (deposited by lenders; borrowed by borrowers).
    pub lend_mint: Account<'info, Mint>,

    /// The authority that will control this pool.
    pub authority: Signer<'info>,

    /// The account that pays for account creation.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: fixed sysvar address — `address` constraint verified against SYSVAR_INSTRUCTIONS_ID.
    #[account(address = Pubkey::new_from_array(SYSVAR_INSTRUCTIONS_ID.to_bytes()))]
    pub sysvar_instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn create_handler(
    ctx: Context<Create>,
    ltv_percent: u8,
    rate_program: Pubkey,
    irm_state: Pubkey,
    feed_program: Pubkey,
    feed_state: Pubkey,
) -> Result<()> {
    // ── Verify a set_value call on the feed precedes this instruction ─────────
    //
    // We scan every instruction that comes *before* the current one in the
    // transaction. We require at least one that:
    //   a) targets the given feed_program
    //   b) has the set_value Anchor discriminator
    //   c) references feed_state as its first account (the feed PDA)
    let sysvar_info = ctx.accounts.sysvar_instructions.to_account_info();
    let current_index = load_current_index_checked(&sysvar_info)? as usize;

    let mut found = false;
    for idx in 0..current_index {
        let ix = match load_instruction_at_checked(idx, &sysvar_info) {
            Ok(ix) => ix,
            Err(_) => break,
        };

        if ix.program_id == feed_program
            && ix.data.len() >= 8
            && ix.data[..8] == SET_VALUE_DISCRIMINATOR
        {
            let feed_matches = ix
                .accounts
                .first()
                .map(|a| a.pubkey == feed_state)
                .unwrap_or(false);

            if feed_matches {
                found = true;
                break;
            }
        }
    }
    require!(found, ErrorCode::FeedSetValueMissing);

    // ── Initialise pool fields ────────────────────────────────────────────────
    let mut pool = ctx.accounts.pool.load_init()?;

    pool.authority = ctx.accounts.authority.key();
    pool.collateral_mint = ctx.accounts.collateral_mint.key();
    pool.lend_mint = ctx.accounts.lend_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.total_collateral_deposited = 0;
    pool.market.total_supply_assets = 0;
    pool.market.total_supply_shares = 0;
    pool.market.total_borrow_assets = 0;
    pool.market.total_borrow_shares = 0;
    pool.market.last_update = Clock::get()?.unix_timestamp;
    pool.market.fee = 0;
    pool.market.assets_in_queue = 0;
    pool.ltv_percent = ltv_percent;
    pool.rate_program = rate_program;
    pool.irm_state = irm_state;
    pool.feed_program = feed_program;
    pool.feed_state = feed_state;
    pool.lp_mint_bump = ctx.bumps.lp_mint;
    // withdrawal_queue is zero-initialised by load_init (head=0, tail=0)

    msg!(
        "Created pool for authority: {} collateral_mint: {} lend_mint: {} lp_mint: {} at slot: {}",
        ctx.accounts.authority.key(),
        ctx.accounts.collateral_mint.key(),
        ctx.accounts.lend_mint.key(),
        ctx.accounts.lp_mint.key(),
        Clock::get()?.slot
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SET_VALUE_DISCRIMINATOR;
    use sha2::{Digest, Sha256};

    fn anchor_discriminator(ix_name: &str) -> [u8; 8] {
        let preimage = format!("global:{ix_name}");
        let hash = Sha256::digest(preimage.as_bytes());
        hash[..8].try_into().unwrap()
    }

    #[test]
    fn set_value_discriminator_is_correct() {
        assert_eq!(
            SET_VALUE_DISCRIMINATOR,
            anchor_discriminator("set_value"),
        );
    }
}

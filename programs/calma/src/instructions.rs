use anchor_lang::prelude::*;
use anchor_spl::token::{MintTo, Transfer};

/// Move tokens out of a vault, signed by the protocol's `["state"]` PDA.
///
/// Seven handlers built this CPI by hand, each re-deriving the same signer seeds
/// around the same `CpiContext`. They had already drifted three ways — two spellings
/// of `token_program.key` and two constructions of the seed array — which is the
/// usual precursor to one of them drifting in a way that matters.
///
/// `state` must be the account the `seeds = [seeds::STATE]` constraint resolved,
/// and `state_bump` its bump; the caller has both from its own `Accounts` struct.
pub fn transfer_from_vault<'info>(
    token_program: Pubkey,
    state: &AccountInfo<'info>,
    state_bump: u8,
    from: AccountInfo<'info>,
    to: AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let seeds: &[&[u8]] = &[::state::seeds::STATE, &[state_bump]];
    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            token_program,
            Transfer {
                from,
                to,
                authority: state.clone(),
            },
            &[seeds],
        ),
        amount,
    )
}

/// Mint tokens under the protocol's `["state"]` PDA authority — the LP-mint
/// counterpart to [`transfer_from_vault`], same signer, same reasoning.
pub fn mint_with_state_authority<'info>(
    token_program: Pubkey,
    state: &AccountInfo<'info>,
    state_bump: u8,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let seeds: &[&[u8]] = &[::state::seeds::STATE, &[state_bump]];
    anchor_spl::token::mint_to(
        CpiContext::new_with_signer(
            token_program,
            MintTo {
                mint,
                to,
                authority: state.clone(),
            },
            &[seeds],
        ),
        amount,
    )
}

pub mod borrow;
pub mod claim_fees;
pub mod create;
pub mod deposit_collateral;
pub mod deposit_lent;
pub mod flash_borrow;
pub mod flash_repay;
pub mod process_queue;
pub mod repay;
pub mod withdraw_collateral;
pub mod withdraw_lent;

pub use borrow::*;
pub use claim_fees::*;
pub use create::*;
pub use deposit_collateral::*;
pub use deposit_lent::*;
pub use flash_borrow::*;
pub use flash_repay::*;
pub use process_queue::*;
pub use repay::*;
pub use withdraw_collateral::*;
pub use withdraw_lent::*;

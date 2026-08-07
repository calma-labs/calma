use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use state::Pool;

use crate::error::ErrorCode;

/// `sha256("global:check")[..8]` — the whitelist ABI's only instruction.
///
/// Hardcoded for the same reason as the rate discriminators in
/// [`crate::hooks::irm`]: encoding this call through the reference `guard`
/// crate's generated helpers would link one guard implementation into the
/// lending program. A guard is call-compatible iff it exposes `check(Pubkey)`
/// taking a single read-only `guard_state` account. Asserted below.
const CHECK_IX: [u8; 8] = [238, 251, 184, 43, 83, 233, 244, 65];

pub fn check_whitelist<'info>(
    guard_program: AccountInfo<'info>,
    guard_state: AccountInfo<'info>,
    authority: Pubkey,
) -> Result<()> {
    let mut data = CHECK_IX.to_vec();
    data.extend_from_slice(authority.as_ref());

    invoke(
        &Instruction {
            program_id: guard_program.key(),
            accounts: vec![AccountMeta::new_readonly(guard_state.key(), false)],
            data,
        },
        &[guard_state],
    )
    .map_err(Into::into)
}

/// Enforce a market's whitelist on `authority`, if it has one.
///
/// The gate is driven entirely by what `Pool` recorded at market creation, never
/// by what the caller chose to pass:
///
///   * open market (`guard_state` is [`Pubkey::default`]) → no check, and any
///     guard accounts supplied are ignored rather than consulted, so a caller
///     cannot introduce a whitelist the market never opted into;
///   * gated market → the accounts are **required**. Omitting them is
///     `GuardRequired`, not a silent pass. That inversion is the whole security
///     property: a gate a caller can skip by leaving out an optional account is
///     not a gate.
///
/// Both the program and the state account must match the pinned values exactly,
/// and both checks are load-bearing:
///
///   * **State.** Checking only that it is owned by a guard program would let a
///     caller substitute a whitelist they had just created and added themselves
///     to — guards are per-authority and permissionless to create.
///   * **Program.** Checking only the state address would let a caller pass
///     their own program alongside the correct state account and have it return
///     `Ok` without ever reading it. `calma` cannot tell the difference: it hands
///     the account over and believes the answer.
///
/// `guard_program` is pinned per market rather than against one canonical
/// program id, matching `feed_program` and `rate_program`. Which guard a market
/// answers to is its creator's choice, recorded once and inspectable by anyone
/// deciding whether to enter.
pub fn enforce_pool_guard<'info>(
    pool: &AccountLoader<'info, Pool>,
    guard_program: &Option<UncheckedAccount<'info>>,
    guard_state: &Option<UncheckedAccount<'info>>,
    authority: Pubkey,
) -> Result<()> {
    let (pool_guard_state, pool_guard_program) = pool.load()?.guard_config();

    if pool_guard_state == Pubkey::default() {
        return Ok(());
    }

    let (Some(guard_program), Some(guard_state)) = (guard_program, guard_state) else {
        return Err(ErrorCode::GuardRequired.into());
    };

    require_keys_eq!(
        guard_program.key(),
        pool_guard_program,
        ErrorCode::InvalidProgramId
    );
    require_keys_eq!(
        guard_state.key(),
        pool_guard_state,
        ErrorCode::InvalidGuardState
    );

    check_whitelist(
        guard_program.to_account_info(),
        guard_state.to_account_info(),
        authority,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn discriminator_matches_anchors_derivation() {
        let mut hasher = Sha256::new();
        hasher.update("global:check");
        let expected: [u8; 8] = hasher.finalize()[..8].try_into().unwrap();
        assert_eq!(CHECK_IX, expected);
    }
}

use anchor_lang::prelude::*;

use crate::error::ErrorCode;

pub fn check_whitelist<'info>(
    guard_program: AccountInfo<'info>,
    guard_state: AccountInfo<'info>,
    authority: Pubkey,
) -> Result<()> {
    guard::cpi::check(
        CpiContext::new(
            guard_program.key(),
            guard::cpi::accounts::Check { guard_state },
        ),
        authority,
    )
}

/// Enforce a market's whitelist on `authority`, if it has one.
///
/// The gate is driven entirely by `pool_guard_state` — the address recorded in
/// `Pool` at market creation — never by what the caller chose to pass:
///
///   * open market (`Pubkey::default`) → no check, and any guard accounts
///     supplied are ignored rather than consulted, so a caller cannot introduce
///     a whitelist the market never opted into;
///   * gated market → the accounts are **required**. Omitting them is
///     `GuardRequired`, not a silent pass. That inversion is the whole security
///     property: a gate a caller can skip by leaving out an optional account is
///     not a gate.
///
/// The state account must equal the pinned address exactly. Checking only that
/// it is owned by the guard program would let a caller substitute a whitelist
/// they had just created and added themselves to — guards are per-authority and
/// permissionless to create.
pub fn enforce_pool_guard<'info>(
    pool_guard_state: Pubkey,
    guard_program: &Option<UncheckedAccount<'info>>,
    guard_state: &Option<UncheckedAccount<'info>>,
    authority: Pubkey,
) -> Result<()> {
    if pool_guard_state == Pubkey::default() {
        return Ok(());
    }

    let (Some(guard_program), Some(guard_state)) = (guard_program, guard_state) else {
        return Err(ErrorCode::GuardRequired.into());
    };

    require!(
        guard_program.key() == guard::ID,
        ErrorCode::InvalidProgramId
    );
    require!(
        guard_state.key() == pool_guard_state,
        ErrorCode::InvalidGuardState
    );

    check_whitelist(
        guard_program.to_account_info(),
        guard_state.to_account_info(),
        authority,
    )
}

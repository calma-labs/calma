use anchor_lang::prelude::*;

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

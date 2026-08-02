use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::{get_return_data, invoke},
};

/// Anchor instruction discriminators for the rate-provider ABI.
///
/// Hardcoded rather than imported. `calma` used to encode these calls with the
/// reference `irm` crate's generated helpers, which meant linking one provider
/// into the lending program — privileging it at compile time over every other
/// provider a market may legitimately choose. The bytes below are
/// `sha256("global:<name>")[..8]`, which is how Anchor derives them; the
/// derivation is asserted against these constants in the tests at the bottom of
/// this file, so a typo cannot go unnoticed.
///
/// Consequences worth stating, since nothing in the type system enforces them
/// any more:
///   * a provider is call-compatible iff its instructions are *named*
///     `borrow_rate` and `check_authority`;
///   * arguments are borsh — `u64` and `Pubkey` respectively;
///   * accounts are `[rate_state, pool]`, neither signer nor writable.
const BORROW_RATE_IX: [u8; 8] = [70, 111, 1, 166, 209, 117, 83, 195];
const CHECK_AUTHORITY_IX: [u8; 8] = [150, 20, 117, 137, 30, 218, 200, 87];

/// Proof that a borrow-rate CPI to the market's rate program has been executed.
/// Construction performs the CPI; holding an instance is proof it succeeded
/// and `rate_bps` is the verified current borrow rate.
pub struct IrmState {
    pub rate_bps: u32,
    pub current_ts: i64,
}

impl IrmState {
    pub fn new<'a>(
        rate_program: AccountInfo<'a>,
        utilization: u64,
        pool_account: AccountInfo<'a>,
        irm_state: AccountInfo<'a>,
    ) -> Result<Self> {
        let mut data = BORROW_RATE_IX.to_vec();
        data.extend_from_slice(&utilization.to_le_bytes());

        invoke(
            &Instruction {
                program_id: rate_program.key(),
                accounts: vec![
                    AccountMeta::new_readonly(irm_state.key(), false),
                    AccountMeta::new_readonly(pool_account.key(), false),
                ],
                data,
            },
            &[irm_state, pool_account],
        )?;

        // Return data is a single global slot, so it must be attributed before
        // it is believed: a stale value left by an earlier CPI in the same
        // transaction would otherwise be read as this provider's answer.
        let (returning_program, bytes) =
            get_return_data().ok_or(crate::error::ErrorCode::MissingRateState)?;
        require_keys_eq!(
            returning_program,
            rate_program.key(),
            crate::error::ErrorCode::MissingRateProgram
        );
        let rate_bps = u32::from_le_bytes(
            bytes
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .ok_or(crate::error::ErrorCode::MissingRateState)?,
        );

        Ok(Self {
            rate_bps,
            current_ts: Clock::get()?.unix_timestamp,
        })
    }
}

/// Assert that `authority` controls the rate curve at `irm_state`.
///
/// Asked over CPI rather than read off the account, because a market's rate
/// program is its own choice and `calma` does not know its layout. Shaped
/// exactly like [`crate::hooks::guard::check_whitelist`]; the reasoning for the
/// check itself lives on the reference `irm::check_authority`.
pub fn check_irm_authority<'a>(
    rate_program: AccountInfo<'a>,
    irm_state: AccountInfo<'a>,
    pool_account: AccountInfo<'a>,
    authority: Pubkey,
) -> Result<()> {
    let mut data = CHECK_AUTHORITY_IX.to_vec();
    data.extend_from_slice(authority.as_ref());

    invoke(
        &Instruction {
            program_id: rate_program.key(),
            accounts: vec![
                AccountMeta::new_readonly(irm_state.key(), false),
                AccountMeta::new_readonly(pool_account.key(), false),
            ],
            data,
        },
        &[irm_state, pool_account],
    )
    .map_err(Into::into)
}

impl math::IrmRate for IrmState {
    fn rate_bps(&self) -> u32 {
        self.rate_bps
    }
    fn current_ts(&self) -> i64 {
        self.current_ts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn discriminator(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}"));
        hasher.finalize()[..8].try_into().unwrap()
    }

    /// The constants above are the only description of the rate ABI that ships
    /// in the program binary. If one drifts, every market silently calls a
    /// different instruction on its provider.
    #[test]
    fn discriminators_match_anchors_derivation() {
        assert_eq!(BORROW_RATE_IX, discriminator("borrow_rate"));
        assert_eq!(CHECK_AUTHORITY_IX, discriminator("check_authority"));
    }
}

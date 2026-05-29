use anchor_lang::prelude::*;
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};

/// Anchor discriminator for `set_value` = sha256("global:set_value")[0..8].
/// Pre-computed: python3 -c "import hashlib; print(list(hashlib.sha256(b'global:set_value').digest()[:8]))"
pub const SET_VALUE_DISCRIMINATOR: [u8; 8] = [253, 214, 48, 201, 100, 201, 227, 219];

/// Oracle price scale factor. A feed value of PRICE_SCALE represents a 1:1 exchange rate
/// (1 collateral token = 1 lend token). Values above/below scale collateral proportionally.
pub const PRICE_SCALE: u128 = 1_000_000;

/// Read the `value` field from a Feed account.
///
/// Feed account layout (Anchor):
///   [0..8]   discriminator
///   [8..40]  authority (Pubkey, 32 bytes)
///   [40..48] value (u64, little-endian)
///   [48]     bump (u8)
pub fn read_feed_price(feed_state_info: &AccountInfo) -> Result<u64> {
    let data = feed_state_info.try_borrow_data()?;
    require!(data.len() >= 48, crate::error::ErrorCode::InvalidAmount);
    Ok(u64::from_le_bytes(
        data[40..48]
            .try_into()
            .map_err(|_| crate::error::ErrorCode::MathOverflow)?,
    ))
}

/// On-chain oracle verifier. Constructed by scanning the sysvar_instructions
/// sysvar for a preceding `set_value` call; holding an instance guarantees the
/// check passed. Converts into [`jbl_state::oracle::OracleState`] for use with
/// `Pool::accrue_interest`.
pub struct OracleState {
    pub feed_program: Pubkey,
    pub feed_state: Pubkey,
    pub current_ts: i64,
}

impl OracleState {
    /// Scan the sysvar_instructions sysvar for a preceding `set_value` call that
    /// targets `feed_state` and originates from `feed_program`. Captures the
    /// current clock timestamp for later use in interest accrual.
    pub fn new(
        sysvar_info: &AccountInfo,
        feed_program: Pubkey,
        feed_state: Pubkey,
    ) -> Result<Self> {
        let current_ts = Clock::get()?.unix_timestamp;
        let current_index = load_current_index_checked(sysvar_info)? as usize;

        let mut found = false;
        for idx in 0..current_index {
            let ix = match load_instruction_at_checked(idx, sysvar_info) {
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
        require!(found, crate::error::ErrorCode::FeedSetValueMissing);

        Ok(Self {
            feed_program,
            feed_state,
            current_ts,
        })
    }
}

impl From<OracleState> for jbl_state::oracle::OracleState {
    fn from(s: OracleState) -> Self {
        Self {
            feed_program: s.feed_program,
            feed_state: s.feed_state,
            current_ts: s.current_ts,
        }
    }
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
        assert_eq!(SET_VALUE_DISCRIMINATOR, anchor_discriminator("set_value"),);
    }
}

use anchor_lang::prelude::*;

/// Oracle data captured at instruction time: which feed was verified and the
/// clock timestamp at the moment of verification. Used as proof that a fresh
/// price exists in this transaction before interest is accrued.
pub struct OracleState {
    pub feed_program: Pubkey,
    pub feed_state: Pubkey,
    pub current_ts: i64,
}

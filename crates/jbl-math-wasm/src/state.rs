//! Zero-copy deserialization of Anchor account bytes into `jbl_state` types.
//!
//! Each `parse_*` function expects the full raw account bytes as transmitted
//! by the server (8-byte Anchor discriminator included).  It strips the
//! discriminator, verifies the remaining length, then casts the bytes directly
//! to the canonical struct type using `bytemuck::pod_read_unaligned`.
//!
//! Both AMD64 (server) and wasm32 (client) are little-endian, so no
//! byte-swapping is required.

use bytemuck::Pod;
use jbl_state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition};

const DISCRIMINATOR: usize = 8;

// ── compile-time size assertions ──────────────────────────────────────────────

const _: () = {
    assert!(core::mem::size_of::<Pool>() == 41_184);
    assert!(core::mem::size_of::<UserPosition>() == 88);
    assert!(core::mem::size_of::<RateHedgeOffer>() == 120);
    assert!(core::mem::size_of::<RateHedgeMatch>() == 112);
};

// ── wrapper types ─────────────────────────────────────────────────────────────

pub struct PoolAccount(pub Pool);
pub struct UserPositionAccount(pub UserPosition);
pub struct RateHedgeOfferAccount(pub RateHedgeOffer);
pub struct RateHedgeMatchAccount(pub RateHedgeMatch);

// ── parsing ───────────────────────────────────────────────────────────────────

fn parse<T: Pod>(account_data: &[u8]) -> Option<T> {
    let body = account_data.get(DISCRIMINATOR..)?;
    if body.len() != core::mem::size_of::<T>() {
        return None;
    }
    Some(bytemuck::pod_read_unaligned(body))
}

impl PoolAccount {
    pub fn from_bytes(account_data: &[u8]) -> Option<Self> {
        parse(account_data).map(Self)
    }
}

impl UserPositionAccount {
    pub fn from_bytes(account_data: &[u8]) -> Option<Self> {
        parse(account_data).map(Self)
    }
}

impl RateHedgeOfferAccount {
    pub fn from_bytes(account_data: &[u8]) -> Option<Self> {
        parse(account_data).map(Self)
    }
}

impl RateHedgeMatchAccount {
    pub fn from_bytes(account_data: &[u8]) -> Option<Self> {
        parse(account_data).map(Self)
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn account_bytes<T: Pod + bytemuck::Zeroable>() -> Vec<u8> {
        let mut v = vec![0u8; DISCRIMINATOR + core::mem::size_of::<T>()];
        v[..DISCRIMINATOR].fill(0xAA);
        v
    }

    #[test]
    fn struct_sizes() {
        assert_eq!(core::mem::size_of::<Pool>(), 41_184);
        assert_eq!(core::mem::size_of::<UserPosition>(), 88);
        assert_eq!(core::mem::size_of::<RateHedgeOffer>(), 120);
        assert_eq!(core::mem::size_of::<RateHedgeMatch>(), 112);
    }

    #[test]
    fn rejects_short_slice() {
        let short = vec![0u8; DISCRIMINATOR + core::mem::size_of::<UserPosition>() - 1];
        assert!(UserPositionAccount::from_bytes(&short).is_none());
    }

    #[test]
    fn rejects_missing_discriminator() {
        let body_only = vec![0u8; core::mem::size_of::<UserPosition>()];
        assert!(UserPositionAccount::from_bytes(&body_only).is_none());
    }

    #[test]
    fn parses_zeroed_user_position() {
        let bytes = account_bytes::<UserPosition>();
        let pos = UserPositionAccount::from_bytes(&bytes)
            .expect("should parse")
            .0;
        assert_eq!(pos.collateral_deposited, 0);
        assert_eq!(pos.debt_shares, 0);
        assert_eq!(pos.bump, 0);
    }

    #[test]
    fn parses_user_position_known_values() {
        let mut bytes = account_bytes::<UserPosition>();
        bytes[DISCRIMINATOR] = 0xAB;
        bytes[DISCRIMINATOR + 31] = 0xCD;
        let cd_off = DISCRIMINATOR + 64;
        bytes[cd_off..cd_off + 8].copy_from_slice(&1_000_000u64.to_le_bytes());
        let ds_off = DISCRIMINATOR + 72;
        bytes[ds_off..ds_off + 8].copy_from_slice(&42u64.to_le_bytes());
        bytes[DISCRIMINATOR + 80] = 7;

        let pos = UserPositionAccount::from_bytes(&bytes)
            .expect("should parse")
            .0;
        assert_eq!(pos.authority.as_ref()[0], 0xAB);
        assert_eq!(pos.authority.as_ref()[31], 0xCD);
        assert_eq!(pos.collateral_deposited, 1_000_000);
        assert_eq!(pos.debt_shares, 42);
        assert_eq!(pos.bump, 7);
    }
}

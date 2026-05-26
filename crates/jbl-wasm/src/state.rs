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
use wasm_bindgen::prelude::*;

const DISCRIMINATOR: usize = 8;

// ── compile-time size assertions ──────────────────────────────────────────────

const _: () = {
    assert!(core::mem::size_of::<Pool>() == 41_264);
    assert!(core::mem::size_of::<UserPosition>() == 88);
    assert!(core::mem::size_of::<RateHedgeOffer>() == 120);
    assert!(core::mem::size_of::<RateHedgeMatch>() == 112);
};

// ── wrapper types ─────────────────────────────────────────────────────────────

/// Wasm-exposed wrapper around a parsed `Pool` account.
#[wasm_bindgen]
pub struct PoolAccount(pub(crate) Pool);
/// Wasm-exposed wrapper around a parsed `UserPosition` account.
#[wasm_bindgen]
pub struct UserPositionAccount(pub(crate) UserPosition);
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

#[wasm_bindgen]
impl PoolAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    pub fn from_bytes(account_data: &[u8]) -> Option<PoolAccount> {
        parse(account_data).map(Self)
    }

    /// Authority pubkey as raw 32 bytes. Use `new PublicKey(pool.authority)` on the JS side.
    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.authority).to_vec()
    }

    /// Collateral mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn collateral_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.collateral_mint).to_vec()
    }

    /// Lend mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn lend_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.lend_mint).to_vec()
    }

    /// LP mint pubkey as raw 32 bytes.
    #[wasm_bindgen(getter)]
    pub fn lp_mint(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.lp_mint).to_vec()
    }

    /// Raw sum of collateral tokens deposited across all positions.
    #[wasm_bindgen(getter)]
    pub fn total_collateral_deposited(&self) -> u64 {
        self.0.total_collateral_deposited
    }

    /// Sum of lend tokens currently deposited.
    #[wasm_bindgen(getter)]
    pub fn total_supply_assets(&self) -> u64 {
        self.0.market.total_supply_assets
    }

    /// Total LP tokens outstanding for the lend side.
    #[wasm_bindgen(getter)]
    pub fn total_supply_shares(&self) -> u64 {
        self.0.market.total_supply_shares
    }

    /// Total borrowed lend tokens outstanding.
    #[wasm_bindgen(getter)]
    pub fn total_borrow_assets(&self) -> u64 {
        self.0.market.total_borrow_assets
    }

    /// Total debt shares outstanding.
    #[wasm_bindgen(getter)]
    pub fn total_borrow_shares(&self) -> u64 {
        self.0.market.total_borrow_shares
    }

    /// Unix timestamp of the last interest accrual.
    #[wasm_bindgen(getter)]
    pub fn last_update(&self) -> i64 {
        self.0.market.last_update
    }

    #[wasm_bindgen(getter)]
    pub fn fee(&self) -> u64 {
        self.0.market.fee
    }

    #[wasm_bindgen(getter)]
    pub fn assets_in_queue(&self) -> u64 {
        self.0.market.assets_in_queue
    }

    /// LTV percent (e.g. 80 means 80%).
    #[wasm_bindgen(getter)]
    pub fn ltv_percent(&self) -> u8 {
        self.0.ltv_percent
    }

    /// LP mint PDA bump seed.
    #[wasm_bindgen(getter)]
    pub fn lp_mint_bump(&self) -> u8 {
        self.0.lp_mint_bump
    }

    /// Total lend tokens committed to pending withdrawals in the on-chain queue.
    pub fn pending_withdrawals(&self) -> u64 {
        self.0.market.assets_in_queue
    }

    /// Borrow rate in basis points for a given utilization (0..10_000).
    pub fn fee_bps(&self, utilization_bps: u16) -> u32 {
        self.0.fee_config.get_fee_bps(utilization_bps as u64)
    }

    /// Slope coefficient (a) for the given curve index (0–3).
    pub fn fee_curve_a(&self, curve: u8) -> i64 {
        self.0.fee_config.curves.get(curve as usize).map_or(0, |c| c.a)
    }

    /// Base rate (b) for the given curve index (0–3).
    pub fn fee_curve_b(&self, curve: u8) -> i64 {
        self.0.fee_config.curves.get(curve as usize).map_or(0, |c| c.b)
    }

    /// Whether the given curve index (0–3) is enabled (non-zero = enabled).
    pub fn fee_curve_enabled(&self, curve: u8) -> u8 {
        self.0.fee_config.curves.get(curve as usize).map_or(0, |c| c.enabled)
    }

    /// Effective utilization in basis points (0..10_000), including pending withdrawals.
    pub fn utilization_bps(&self) -> u16 {
        let total_lend = self.0.market.total_supply_assets;
        if total_lend == 0 {
            return 0;
        }
        let effective_borrowed =
            self.0.market.total_borrow_assets.saturating_add(self.pending_withdrawals());
        ((effective_borrowed as u128 * 10_000 / total_lend as u128).min(10_000)) as u16
    }

    /// Borrow APY in basis points at the current utilization.
    pub fn borrow_apy_bps(&self) -> u32 {
        self.0.fee_config.get_fee_bps(self.utilization_bps() as u64)
    }

    /// Supply APY in basis points: borrow_apy × (total_borrow_assets / total_supply_assets).
    pub fn supply_apy_bps(&self) -> u32 {
        let total_lend = self.0.market.total_supply_assets;
        if total_lend == 0 {
            return 0;
        }
        let borrow_apy = self.borrow_apy_bps() as u128;
        let total_borrowed = self.0.market.total_borrow_assets as u128;
        ((borrow_apy * total_borrowed) / total_lend as u128) as u32
    }

    /// Available liquidity in raw token units (lend deposited minus effective borrowed).
    pub fn available_liquidity(&self) -> u64 {
        let effective_borrowed =
            self.0.market.total_borrow_assets.saturating_add(self.pending_withdrawals());
        self.0.market.total_supply_assets.saturating_sub(effective_borrowed)
    }
}

#[wasm_bindgen]
impl UserPositionAccount {
    /// Parse from raw Anchor account bytes (8-byte discriminator included).
    pub fn from_bytes(account_data: &[u8]) -> Option<UserPositionAccount> {
        parse(account_data).map(Self)
    }

    /// Authority pubkey as raw 32 bytes (`Uint8Array` in JS).
    /// Use `new PublicKey(pos.authority)` on the JS side to reconstruct.
    #[wasm_bindgen(getter)]
    pub fn authority(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.authority).to_vec()
    }

    /// Pool pubkey as raw 32 bytes (`Uint8Array` in JS).
    /// Use `new PublicKey(pos.pool)` on the JS side to reconstruct.
    #[wasm_bindgen(getter)]
    pub fn pool(&self) -> Vec<u8> {
        bytemuck::bytes_of(&self.0.pool).to_vec()
    }

    /// Raw collateral deposited in token base units.
    #[wasm_bindgen(getter)]
    pub fn collateral_deposited(&self) -> u64 {
        self.0.collateral_deposited
    }

    /// Raw debt shares held by this position.
    #[wasm_bindgen(getter)]
    pub fn debt_shares(&self) -> u64 {
        self.0.debt_shares
    }

    /// PDA bump seed.
    #[wasm_bindgen(getter)]
    pub fn bump(&self) -> u8 {
        self.0.bump
    }

    /// Returns `true` if collateral has been deposited.
    pub fn has_collateral(&self) -> bool {
        self.0.collateral_deposited > 0
    }

    /// Returns `true` if there is an active borrow (debt shares > 0).
    pub fn has_debt(&self) -> bool {
        self.0.debt_shares > 0
    }

    /// Raw debt amount in lend-token base units, derived from shares and pool totals.
    /// Returns 0 when `total_debt_shares` is 0.
    /// Delegates to `jbl_math::shares_to_amount` (ceiling division).
    pub fn debt_amount(&self, total_borrowed: u64, total_debt_shares: u64) -> u64 {
        jbl_math::shares_to_amount(self.0.debt_shares, total_borrowed, total_debt_shares)
            .unwrap_or(0)
    }

    /// Maximum borrowable amount in raw lend-token units.
    /// Delegates to `jbl_math::max_borrowable` — same formula as the on-chain LTV check.
    pub fn max_borrowable(&self, ltv_percent: u8) -> u64 {
        jbl_math::max_borrowable(self.0.collateral_deposited, ltv_percent)
    }

    /// Compute Loan-to-Value (LTV) ratio in basis points.
    pub fn ltv(&self, total_borrowed: u64, total_debt_shares: u64) -> Option<u32> {
        let debt = self.debt_amount(total_borrowed, total_debt_shares);
        jbl_math::compute_ltv(debt, self.0.collateral_deposited)
    }

    /// Compute Health Factor in basis points (1.0 = 10,000).
    pub fn health_factor(
        &self,
        total_borrowed: u64,
        total_debt_shares: u64,
        ltv_percent: u8,
    ) -> Option<u32> {
        let debt = self.debt_amount(total_borrowed, total_debt_shares);
        jbl_math::compute_health_factor(self.0.collateral_deposited, ltv_percent, debt)
    }

    /// Compute Liquidation "Price" (ratio) in basis points.
    pub fn liq_price(
        &self,
        total_borrowed: u64,
        total_debt_shares: u64,
        ltv_percent: u8,
    ) -> Option<u32> {
        let debt = self.debt_amount(total_borrowed, total_debt_shares);
        jbl_math::compute_liquidation_threshold(debt, self.0.collateral_deposited, ltv_percent)
    }

    /// Human-readable collateral amount as a decimal string (e.g. `"1234.5678"`).
    /// Trailing zeros are trimmed; at most 6 decimal places are shown.
    pub fn format_collateral(&self, decimals: u8) -> String {
        format_token_amount(self.0.collateral_deposited, decimals)
    }

    /// Human-readable debt amount as a decimal string.
    /// Shares are resolved against `total_borrowed` / `total_debt_shares` first.
    pub fn format_debt(&self, total_borrowed: u64, total_debt_shares: u64, decimals: u8) -> String {
        let amount = self.debt_amount(total_borrowed, total_debt_shares);
        format_token_amount(amount, decimals)
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

// ── formatting helpers ────────────────────────────────────────────────────────

/// Converts a raw token amount to a human-readable decimal string.
/// Trims trailing fractional zeros and caps output at 6 decimal places.
///
/// Examples (decimals = 6):
///   1_000_000 → "1"
///   1_500_000 → "1.5"
///   1_234_567 → "1.234567"
///   1_234_560 → "1.23456"
fn format_token_amount(raw: u64, decimals: u8) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let scale = 10u64.pow(decimals as u32);
    let whole = raw / scale;
    let frac = raw % scale;
    if frac == 0 {
        return whole.to_string();
    }
    // Left-pad fractional part to `decimals` digits, then trim to ≤ 6 places.
    let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
    let max_places = 6_usize.min(decimals as usize);
    let trimmed = frac_str[..max_places].trim_end_matches('0');
    if trimmed.is_empty() {
        return whole.to_string();
    }
    format!("{}.{}", whole, trimmed)
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
        assert_eq!(core::mem::size_of::<Pool>(), 41_264);
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

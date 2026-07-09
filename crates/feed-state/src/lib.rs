use anchor_lang::prelude::*;

// The feed types are bound to this program — declare the same ID so the
// #[account] macro can resolve `ID` in the crate root.
declare_id!("orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4");

pub const PRICE_SCALE: u128 = 1_000_000;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PriceSource {
    Manual = 0,
    Pyth = 1,
    PythPush = 2,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedState {
    pub collateral_price: u64,
    pub lend_price: u64,
    pub last_updated_ts: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedConfig {
    pub authority: Pubkey,
    pub source: PriceSource,
    pub bump: u8,
    pub _pad: [u8; 2],
    pub max_pyth_age_secs: u32,
    pub collateral_feed_id: [u8; 32],
    pub lend_feed_id: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedData {
    pub collateral_mint: Pubkey,
    pub lend_mint: Pubkey,
    pub collateral_decimals: u8,
    pub lend_decimals: u8,
}

/// Optional validation gates applied to Pyth updates. `0` is the "disabled"
/// sentinel for every field so a zero-init `FeedRules` reproduces the legacy
/// behavior. Fixed at feed creation; `set_from_pyth` is the only enforcement
/// site — Manual updates ignore rules entirely.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FeedRules {
    /// Reject if `conf / price` exceeds this many basis points.
    pub max_conf_bps: u16,
    /// Reject if `|new − last| / last` exceeds `bps_per_hour × elapsed_hours`,
    /// with the effective ceiling clamped at 10_000 bps.
    pub max_deviation_bps_per_hour: u16,
    /// Reject if `|price − ema_price| / ema_price` exceeds this many bps.
    pub ema_divergence_bps: u16,
    /// Reject if normalized price is below this floor (in PRICE_SCALE units).
    pub min_price: u64,
    /// Reject if normalized price is above this ceiling (in PRICE_SCALE units).
    pub max_price: u64,
    pub _reserved: [u8; 8],
}

#[account]
#[derive(Copy)]
pub struct Feed {
    pub state: FeedState,
    pub config: FeedConfig,
    pub data: FeedData,
    pub rules: FeedRules,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct FeedSnapshot {
    pub ratio: u64,
    pub last_updated_ts: i64,
}

pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use error::ErrorCode;
pub use instructions::*;
pub use irm_state::*;

declare_id!("irmdacogiedKeCEBh72FJx4aoixyaByqGikTkxGifUk");

#[program]
pub mod irm {
    use super::*;

    pub fn borrow_rate(ctx: Context<BorrowRate>, utilization_bps: u64) -> Result<u32> {
        borrow_rate::handler(ctx, utilization_bps)
    }

    pub fn check_authority(ctx: Context<CheckAuthority>, authority: Pubkey) -> Result<()> {
        check_authority::handler(ctx, authority)
    }

    pub fn initialize(ctx: Context<Initialize>, points: Vec<RatePointArgs>) -> Result<()> {
        initialize::handler(ctx, points)
    }

    pub fn set_fee_points(ctx: Context<SetFeePoints>, points: Vec<RatePointArgs>) -> Result<()> {
        set_fee_points::handler(ctx, points)
    }
}

pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use error::ErrorCode;
pub use instructions::*;
pub use jbl_irm::*;

declare_id!("3o6qREQVSmDwiny77YaTWdDhVKC9YRkQEX7eydid9pFt");

#[program]
pub mod irm {
    use super::*;

    pub fn borrow_rate(ctx: Context<BorrowRate>, utilization_bps: u64) -> Result<u32> {
        borrow_rate::handler(ctx, utilization_bps)
    }

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        initialize::handler(ctx)
    }

    pub fn set_fee_curve(ctx: Context<SetFeeCurve>, index: u8, curve: CurveArgs) -> Result<()> {
        set_fee_curve::handler(ctx, index, curve)
    }
}

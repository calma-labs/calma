pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;

declare_id!("3zq3hPKkE9SPpkbMcWhaCawWDPXGfkYtboyLE48qKVBC");

#[program]
pub mod irm {
    use super::*;

    pub fn borrow_rate(ctx: Context<BorrowRate>, utilization_bps: u32) -> Result<u32> {
        borrow_rate::handler(ctx, utilization_bps)
    }

    pub fn initialize(ctx: Context<Initialize>, a: u64, b: u64) -> Result<()> {
        initialize::handler(ctx, a, b)
    }
}

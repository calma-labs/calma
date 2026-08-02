pub mod constants;
pub mod error;
pub mod hooks;
pub mod instructions;
pub mod state;
pub mod withdrawal_queue;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::ErrorCode;
pub use instructions::*;
pub use state::*;

declare_id!("c1md3yLhwBxREDRc2HcX4ivTigoyJ3No3DehkzjaPT8");

#[program]
pub mod calma {
    use super::*;

    pub fn create(ctx: Context<Create>, ltv_percent: u8) -> Result<()> {
        create_handler(ctx, ltv_percent)
    }

    pub fn deposit_collateral(ctx: Context<DepositCollateral>, amount: u64) -> Result<()> {
        deposit_collateral_handler(ctx, amount)
    }

    pub fn borrow<'a>(ctx: Context<'a, Borrow<'a>>, amount: u64) -> Result<()> {
        borrow_handler(ctx, amount)
    }

    pub fn repay<'a>(ctx: Context<'a, Repay<'a>>, amount: u64) -> Result<()> {
        repay_handler(ctx, amount)
    }

    pub fn withdraw_collateral<'a>(
        ctx: Context<'a, WithdrawCollateral<'a>>,
        amount: u64,
    ) -> Result<()> {
        withdraw_collateral_handler(ctx, amount)
    }

    pub fn process_queue_entry(ctx: Context<ProcessQueueEntry>) -> Result<()> {
        process_queue_entry_handler(ctx)
    }

    pub fn deposit_lent(ctx: Context<DepositLent>, amount: u64) -> Result<()> {
        deposit_lent_handler(ctx, amount)
    }

    pub fn withdraw_lent(ctx: Context<WithdrawLent>, shares: u64) -> Result<()> {
        withdraw_lent_handler(ctx, shares)
    }

    pub fn claim_fees(ctx: Context<ClaimFees>) -> Result<()> {
        claim_fees_handler(ctx)
    }

    pub fn flash_borrow(ctx: Context<FlashBorrow>, amount: u64) -> Result<()> {
        flash_borrow_handler(ctx, amount)
    }

    pub fn flash_repay(ctx: Context<FlashRepay>, amount: u64) -> Result<()> {
        flash_repay_handler(ctx, amount)
    }
}

use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct BorrowRate<'info> {
    /// CHECK: not used
    pub irm_state: UncheckedAccount<'info>,
    /// CHECK: not used
    pub pool: UncheckedAccount<'info>,
}

pub fn handler(_ctx: Context<BorrowRate>) -> Result<u64> {
    Ok(100)
}

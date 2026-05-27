use anchor_lang::prelude::*;

use crate::fees::PiecewiseLinearModel;

#[account(zero_copy)]
pub struct IrmConfig {
    pub pool: Pubkey,
    pub model: PiecewiseLinearModel,
    pub authority: Pubkey,
    pub bump: u8,
    _pad: [u8; 7],
}

use anchor_lang::prelude::*;
use irm_state::{IrmState, RatePoint, MAX_POINTS};

use crate::error::ErrorCode;

/// One rate curve point in the wire format.  Mirrors `RatePoint` but without
/// zero-copy padding (Anchor-serialized).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct RatePointArgs {
    pub util_bps: u16,
    pub rate_bps: u32,
}

#[derive(Accounts)]
pub struct SetFeePoints<'info> {
    /// Seeded on the **explicit `pool` account**, matching
    /// [`BorrowRate`](super::borrow_rate::BorrowRate).
    ///
    /// Seeding it on `irm_state.load()?.pool` instead — reading the seed out of
    /// the very account being derived — verifies fine on-chain but is a
    /// self-referential PDA the client cannot reproduce: Anchor's IDL advertises
    /// it as auto-resolvable, so generated clients refuse an explicitly passed
    /// `irm_state` yet cannot derive it either without fetching the account
    /// first. The guarantee is unchanged (the derivation still proves this state
    /// belongs to `pool`); only the seed source moves to something a caller
    /// already holds.
    ///
    /// Deliberately **no `has_one = pool`**. The seeds already prove the
    /// relationship — re-deriving `["irm_config", pool]` with the stored bump and
    /// requiring it to equal the account passed is exactly the statement
    /// `has_one` would add. Including it also makes the two accounts mutually
    /// resolvable in the generated IDL (`irm_state` from `pool` via the seeds,
    /// `pool` from `irm_state` via the relation), and Anchor's TypeScript client
    /// then treats *both* as auto-derived and rejects either being passed.
    #[account(
        mut,
        seeds = [::irm_state::IRM_CONFIG_SEED, pool.key().as_ref()],
        bump = irm_state.load()?.bump,
        constraint = irm_state.load()?.authority == authority.key() @ ErrorCode::Unauthorized,
    )]
    pub irm_state: AccountLoader<'info, IrmState>,
    /// CHECK: verified as the seed `irm_state` is derived from; a mismatched
    /// `pool` yields a different PDA and fails the seeds check.
    pub pool: UncheckedAccount<'info>,
    pub authority: Signer<'info>,
}

pub(crate) fn handler(ctx: Context<SetFeePoints>, points: Vec<RatePointArgs>) -> Result<()> {
    // All validation happens here (write time). The reader (`get_fee_bps`)
    // trusts these invariants and skips any runtime checks.
    irm_state::validate_rate_points(
        &points.iter().map(|p| (p.util_bps, p.rate_bps)).collect::<Vec<_>>(),
    )
    .map_err(|e| match e {
        irm_state::RatePointError::InvalidPointList => ErrorCode::InvalidPointList,
        irm_state::RatePointError::RateTooHigh => ErrorCode::RateTooHigh,
    })?;
    let mut state = ctx.accounts.irm_state.load_mut()?;
    let mut buf = [RatePoint::default(); MAX_POINTS];
    for (i, p) in points.iter().enumerate() {
        buf[i] = RatePoint::new(p.util_bps, p.rate_bps);
    }
    state.model.points = buf;
    state.model.len = points.len() as u8;
    Ok(())
}

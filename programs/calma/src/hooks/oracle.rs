use anchor_lang::prelude::*;
use interface::PriceFeedHeader;

/// Read and validate this market's price account.
///
/// There is no CPI here. `calma` used to call `feed::get_state`, which pinned
/// the protocol to one oracle program at compile time — `Account<'info, Feed>`
/// takes its owner check from the defining crate's ID. Markets now record which
/// program and which account they price against, and the price is read straight
/// out of that account's [`PriceFeedHeader`] prefix. Any program can serve as an
/// oracle so long as it writes that prefix.
///
/// Both pinned values come from `Pool`, never from the caller: `pool_feed_state`
/// fixes *which* account and `pool_feed_program` fixes *who may have written
/// it*. See `interface::read_price_feed` for why neither alone suffices.
///
/// The returned header is itself the `math::Oracle`, so it goes straight into
/// `Core::with_oracle`.
pub fn read_feed(
    feed_state: &UncheckedAccount,
    pool_feed_state: Pubkey,
    pool_feed_program: Pubkey,
) -> Result<PriceFeedHeader> {
    let header = interface::read_price_feed(
        &feed_state.to_account_info(),
        pool_feed_state,
        pool_feed_program,
    )?;

    // A zero lend price yields no ratio at all, so `price()` would report 0 —
    // indistinguishable from "the collateral is worthless" and silently
    // collapsing every position's borrow capacity to nothing. Refuse instead, so
    // an unwritten or broken feed reads as an error rather than as a valuation.
    require!(header.lend_price > 0, crate::error::CalmaError::ZeroPrice);
    require!(
        !header.is_stale_at(Clock::get()?.unix_timestamp),
        crate::error::CalmaError::StaleOracle
    );

    Ok(header)
}

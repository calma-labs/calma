mod common;
use common::fixture::Fixture;

use anchor_lang::prelude::Pubkey;
use calma::state::Pool;
use solana_signer::Signer;

/// `create` succeeds when the market authority is not the payer.
///
/// Worth its own test because the authority is not just a field: `calma::create`
/// CPIs into the rate program to assert the same authority controls the curve, so
/// a market whose authority differs from whoever funded it exercises a path the
/// payer-is-authority tests never reach.
#[test]
fn test_create() {
    let f = Fixture::builder().separate_authority().build();

    assert_ne!(
        f.authority(),
        f.payer.pubkey(),
        "fixture should have produced a distinct authority"
    );

    let account = f.svm.get_account(&f.pool).expect("pool account exists");
    assert_eq!(account.owner, calma::id());

    let pool: &Pool = bytemuck::from_bytes(&account.data[8..8 + std::mem::size_of::<Pool>()]);
    assert_eq!(pool.authority, f.authority());
    assert_eq!(pool.collateral_mint, f.col_mint);
    assert_eq!(pool.lend_mint, f.lend_mint);
    assert_eq!(pool.market.ltv_percent, f.ltv_percent);

    // The provider ids the market is pinned to for life.
    assert_eq!(pool.rate_program, f.rate_program);
    assert_eq!(pool.irm_state, f.rate_state);
    assert_eq!(pool.feed_state, f.feed);
    assert_eq!(pool.feed_program, feed::id());
    assert_eq!(pool.guard_program, Pubkey::default(), "no guard was passed");
}

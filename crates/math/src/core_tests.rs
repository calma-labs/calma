//! Unit tests for `Core` (kept as a crate submodule via `#[path]` in
//! `core.rs` so they retain access to crate-private items like
//! `SECONDS_PER_YEAR`).

use super::*;

#[derive(Clone, Copy)]
struct TestMarket {
    supply_a: u64,
    supply_s: u64,
    borrow_a: u64,
    borrow_s: u64,
    last_update: i64,
    aiq: u64,
    ltv: u8,
    fee: u64,
    fee_shares: u64,
}
impl Default for TestMarket {
    fn default() -> Self {
        TestMarket {
            supply_a: 0, supply_s: 0, borrow_a: 0, borrow_s: 0,
            last_update: 0, aiq: 0, ltv: 0, fee: 0, fee_shares: 0,
        }
    }
}
impl Market for TestMarket {
    fn total_supply_assets(&self) -> u64 { self.supply_a }
    fn total_supply_shares(&self) -> u64 { self.supply_s }
    fn total_borrow_assets(&self) -> u64 { self.borrow_a }
    fn total_borrow_shares(&self) -> u64 { self.borrow_s }
    fn last_update(&self) -> i64 { self.last_update }
    fn fee(&self) -> u64 { self.fee }
    fn accrued_fee_shares(&self) -> u64 { self.fee_shares }
    fn assets_in_queue(&self) -> u64 { self.aiq }
    fn ltv_percent(&self) -> u8 { self.ltv }
    fn total_supply_assets_mut(&mut self) -> &mut u64 { &mut self.supply_a }
    fn total_supply_shares_mut(&mut self) -> &mut u64 { &mut self.supply_s }
    fn accrued_fee_shares_mut(&mut self) -> &mut u64 { &mut self.fee_shares }
    fn assets_in_queue_mut(&mut self) -> &mut u64 { &mut self.aiq }
    fn total_borrow_assets_mut(&mut self) -> &mut u64 { &mut self.borrow_a }
    fn total_borrow_shares_mut(&mut self) -> &mut u64 { &mut self.borrow_s }
    fn last_update_mut(&mut self) -> &mut i64 { &mut self.last_update }
}

#[derive(Clone, Copy)]
struct TestPosition { collateral: u64, debt_shares: u64 }
impl Position for TestPosition {
    fn collateral_deposited(&self) -> u64 { self.collateral }
    fn debt_shares(&self) -> u64 { self.debt_shares }
    fn collateral_deposited_mut(&mut self) -> &mut u64 { &mut self.collateral }
    fn debt_shares_mut(&mut self) -> &mut u64 { &mut self.debt_shares }
}

#[derive(Clone, Copy)]
struct TestIrm { rate_bps: u32, current_ts: i64 }
impl IrmRate for TestIrm {
    fn rate_bps(&self) -> u32 { self.rate_bps }
    fn current_ts(&self) -> i64 { self.current_ts }
}

#[derive(Clone, Copy)]
struct TestOracle { price: u64 }
impl Oracle for TestOracle {
    fn price(&self) -> u64 { self.price }
}

/// Replays the exact on-chain full-repay sequence against the live devnet
/// state that reported `MathOverflow`: pool 2a2xed at util≈24.75%
/// (borrow_a=2_475_005, borrow_s=2_474_904), position debt_shares=2_474_904,
/// IRM rate=154 bps, 50 s elapsed. Must NOT overflow — proving the reported
/// error came from a stale deployed binary, not current source.
#[test]
fn full_repay_on_reported_devnet_state_does_not_overflow() {
    let market = TestMarket {
        supply_a: 10_000_000,
        supply_s: 10_000_000,
        borrow_a: 2_475_005,
        borrow_s: 2_474_904,
        last_update: 1_784_229_466,
        aiq: 0,
        ltv: 97,
        fee: 0,
        fee_shares: 0,
    };
    let position = TestPosition { collateral: 1_000_000_000, debt_shares: 2_474_904 };
    let irm = TestIrm { rate_bps: 154, current_ts: 1_784_229_516 };

    let mut core = Core::new(market)
        .with_irm(irm)
        .with_position(position)
        .accrue_interest()
        .expect("accrue_interest must not overflow");

    // Full repay: on-chain caller passes u64::MAX; Core caps at total_due.
    let (repaid, burned) = match core.repay(u64::MAX, |_| Ok::<(), ()>(())) {
        Ok(v) => v,
        Err(_) => panic!("repay must not overflow"),
    };

    assert_eq!(burned, 2_474_904, "burns entire debt");
    assert_eq!(core.position.debt_shares(), 0, "position cleared");
    assert_eq!(core.market.total_borrow_shares(), 0, "pool borrow shares cleared");
    // Interest accrued over 50 s at 154 bps on 2_475_005 rounds up to 1 unit.
    assert_eq!(repaid, 2_475_006);
    assert_eq!(core.market.total_borrow_assets(), 0, "pool borrow assets cleared");
}

/// H1 regression: accruing interest must credit the supply side so lenders
/// earn yield. 100% APR for one year on 1_000_000 borrowed accrues 1_000_000
/// interest, added to BOTH borrow and supply assets (books stay balanced),
/// doubling the LP share price.
#[test]
fn accrue_interest_credits_lenders_supply_side() {
    let year = crate::SECONDS_PER_YEAR as i64;
    let market = TestMarket {
        supply_a: 1_000_000,
        supply_s: 1_000_000,
        borrow_a: 1_000_000,
        borrow_s: 1_000_000,
        last_update: 1_000,
        aiq: 0,
        ltv: 75,
        fee: 0,
        fee_shares: 0,
    };
    let irm = TestIrm { rate_bps: 10_000, current_ts: 1_000 + year };

    let core = Core::new(market)
        .with_irm(irm)
        .accrue_interest()
        .expect("accrue must not overflow");

    assert_eq!(core.market.total_borrow_assets(), 2_000_000);
    assert_eq!(
        core.market.total_supply_assets(),
        2_000_000,
        "lenders must be credited the accrued interest"
    );
    // 1_000_000 LP shares now redeem for 2_000_000 assets — yield accrued.
    assert_eq!(core.calc_lend_for_shares(1_000_000), Some(2_000_000));
}

/// Protocol fee: with `fee` = 1_000 bps (10%), a 1_000_000 interest accrual
/// gives lenders 90% and mints the protocol fee shares worth the other 10%.
/// Books stay balanced: supply_assets holds the full interest.
#[test]
fn accrue_interest_mints_protocol_fee_shares() {
    let year = crate::SECONDS_PER_YEAR as i64;
    let market = TestMarket {
        supply_a: 1_000_000,
        supply_s: 1_000_000,
        borrow_a: 1_000_000,
        borrow_s: 1_000_000,
        last_update: 1_000,
        fee: 1_000, // 10% of interest to the protocol
        ..Default::default()
    };
    let irm = TestIrm { rate_bps: 10_000, current_ts: 1_000 + year };

    let core = Core::new(market)
        .with_irm(irm)
        .accrue_interest()
        .expect("accrue must not overflow");

    // Full interest (1_000_000) credited to supply; books balanced.
    assert_eq!(core.market.total_supply_assets(), 2_000_000);
    // feeAmount = 100_000; feeShares = 100_000 × 1_000_000 / (2_000_000 −
    // 100_000) = 100_000_000_000 / 1_900_000 = 52_631.
    let fee_shares = core.market.accrued_fee_shares();
    assert_eq!(fee_shares, 52_631);
    assert_eq!(core.market.total_supply_shares(), 1_000_000 + fee_shares);
    // The protocol's shares are worth ~fee_amount (10% of interest). Rounding
    // (fee shares floored, valuation floored) makes it under-collect by a few
    // units — the protocol-safe direction, never over.
    let protocol_value = core.calc_lend_for_shares(fee_shares).unwrap();
    assert!(
        protocol_value <= 100_000 && 100_000 - protocol_value <= 3,
        "protocol ≈ 10% of interest, got {protocol_value}"
    );
    // Lenders' original shares keep the remaining ~90% (plus principal), and
    // the two claims together never exceed total supply assets.
    let lender_value = core.calc_lend_for_shares(1_000_000).unwrap();
    assert!(lender_value.abs_diff(1_900_000) <= 2, "lenders ≈ 90% + principal");
    assert!(protocol_value + lender_value <= 2_000_000, "no over-issuance");
}

/// Fee edge case: when the accrued interest is so small the fee rounds to
/// zero, no fee shares are minted (and nothing panics). Lenders still get the
/// full interest.
#[test]
fn accrue_interest_tiny_fee_rounds_to_zero_shares() {
    let year = crate::SECONDS_PER_YEAR as i64;
    let market = TestMarket {
        supply_a: 1_000_000,
        supply_s: 1_000_000,
        borrow_a: 1, // 100% APR on 1 unit for a year = 1 unit interest
        borrow_s: 1,
        last_update: 1_000,
        fee: 2_000, // 20% of 1 = 0.2 → floors to 0
        ..Default::default()
    };
    let irm = TestIrm { rate_bps: 10_000, current_ts: 1_000 + year };

    let core = Core::new(market)
        .with_irm(irm)
        .accrue_interest()
        .expect("accrue must not overflow");

    assert_eq!(core.market.accrued_fee_shares(), 0, "no fee shares for sub-unit fee");
    assert_eq!(core.market.total_supply_assets(), 1_000_001, "lenders still credited");
    assert_eq!(core.market.total_supply_shares(), 1_000_000, "no shares minted");
}

/// H2 regression: `borrow_with_fee` must gate LTV on the full recorded debt
/// (`total_debt_amount` = principal + fee), not the pre-fee principal, or the
/// fee lets a position exceed max LTV. Capacity here is
/// 1_000_000 × 1.0 × 75% = 750_000.
#[test]
fn borrow_with_fee_gates_ltv_on_total_debt() {
    let market = TestMarket {
        supply_a: 10_000_000,
        supply_s: 10_000_000,
        borrow_a: 0,
        borrow_s: 0,
        last_update: 1_000,
        aiq: 0,
        ltv: 75,
        fee: 0,
        fee_shares: 0,
    };
    let position = TestPosition { collateral: 1_000_000, debt_shares: 0 };
    let irm = TestIrm { rate_bps: 0, current_ts: 1_000 };
    let oracle = TestOracle { price: PRICE_SCALE as u64 };
    let fresh = || {
        Core::new(market)
            .with_position(position)
            .with_oracle(oracle)
            .with_irm(irm)
            .accrue_interest()
            .expect("accrue must not overflow")
    };

    // Principal 750_000 alone fits capacity, but principal + 1 fee exceeds it
    // and must be rejected — the fee is part of the debt.
    let mut over = fresh();
    assert!(
        matches!(
            over.borrow_with_fee(750_000, 750_001, |_| Ok::<(), ()>(())),
            Err(MathError::Undercollateralized)
        ),
        "fee pushing debt over capacity must be Undercollateralized"
    );

    // Total debt exactly at capacity (principal + fee = 750_000) succeeds.
    let mut at = fresh();
    assert!(
        at.borrow_with_fee(749_999, 750_000, |_| Ok::<(), ()>(())).is_ok(),
        "total debt at exact capacity must succeed"
    );
}

/// `max_borrow_capacity` saturates rather than erroring: at `u64::MAX`
/// collateral and price it clamps to `u64::MAX` instead of overflowing, so a
/// large-but-valid position is never bricked by the capacity gate.
#[test]
fn max_borrow_capacity_saturates_at_u64_max() {
    let market = TestMarket {
        supply_a: 0,
        supply_s: 0,
        borrow_a: 0,
        borrow_s: 0,
        last_update: 0,
        aiq: 0,
        ltv: 75,
        fee: 0,
        fee_shares: 0,
    };
    let position = TestPosition { collateral: 0, debt_shares: 0 };
    let core = Core::new(market).with_position(position);
    assert_eq!(
        core.max_borrow_capacity(u64::MAX, u64::MAX),
        u64::MAX,
        "overflowing capacity must clamp to u64::MAX"
    );
    // A normal case still computes exactly: 1_000_000 × 1.0 × 75% = 750_000.
    assert_eq!(core.max_borrow_capacity(1_000_000, PRICE_SCALE as u64), 750_000);
}

/// Borrow capacity boundary via the real `Core::borrow` op (the same gate the
/// program enforces): capacity =
/// collateral × oracle_price / PRICE_SCALE × ltv / 100.
/// Here 1_000_000 collateral × 1.0 price × 75% = 750_000 lend units.
/// Borrowing exactly the capacity succeeds; one unit more is rejected
/// `Undercollateralized`.
#[test]
fn borrow_at_capacity_succeeds_one_over_fails() {
    let market = TestMarket {
        supply_a: 10_000_000,
        supply_s: 10_000_000,
        borrow_a: 0,
        borrow_s: 0,
        last_update: 1_784_229_466,
        aiq: 0,
        ltv: 75,
        fee: 0,
        fee_shares: 0,
    };
    let position = TestPosition { collateral: 1_000_000, debt_shares: 0 };
    let irm = TestIrm { rate_bps: 154, current_ts: 1_784_229_516 };
    let oracle = TestOracle { price: PRICE_SCALE as u64 };

    let fresh = || {
        Core::new(market)
            .with_position(position)
            .with_oracle(oracle)
            .with_irm(irm)
            .accrue_interest()
            .expect("accrue must not overflow")
    };

    const CAPACITY: u64 = 750_000;

    // Exactly at capacity: succeeds, minting the first-borrow 1:1 shares.
    let mut at_cap = fresh();
    match at_cap.borrow(CAPACITY, |_| Ok::<(), ()>(())) {
        Ok(shares) => assert_eq!(shares, CAPACITY, "first borrow mints 1:1 shares"),
        Err(_) => panic!("borrow at exact capacity must succeed"),
    }

    // One unit over capacity: rejected by the LTV gate.
    let mut over_cap = fresh();
    assert!(
        matches!(
            over_cap.borrow(CAPACITY + 1, |_| Ok::<(), ()>(())),
            Err(MathError::Undercollateralized)
        ),
        "borrowing capacity + 1 must be Undercollateralized"
    );
}

// ── settle_hedge ──────────────────────────────────────────────────────────

/// Builds an `Accrued` core with a zero-elapsed IRM (no interest) so the
/// settle-hedge math is exercised in isolation.
fn accrued_core(
    market: TestMarket,
    position: TestPosition,
) -> Core<TestMarket, TestIrm, TestPosition, (), Accrued> {
    let irm = TestIrm { rate_bps: 0, current_ts: 0 };
    Core::new(market)
        .with_position(position)
        .with_irm(irm)
        .accrue_interest()
        .expect("accrue must not overflow")
}

/// settle_hedge closes the borrower's floating debt (`initial_shares`),
/// re-borrows the fixed `borrow_amount`, removes the upfront fee from supply,
/// and pays the floating excess + fee out. No oracle/LTV gate is involved
/// (M4 invariant) — the borrow value here (1.1× share price) is settled fully.
#[test]
fn settle_hedge_rolls_debt_and_pays_excess_and_fee() {
    use std::cell::Cell;
    let market = TestMarket {
        supply_a: 2_000_000,
        supply_s: 2_000_000,
        borrow_a: 1_100_000,
        borrow_s: 1_000_000, // share price 1.1
        ltv: 75,
        ..Default::default()
    };
    let position = TestPosition { collateral: 10_000_000, debt_shares: 1_000_000 };
    let mut core = accrued_core(market, position);

    let excess = Cell::new(0u64);
    let fee = Cell::new(0u64);
    let (current_value, new_shares) = match core.settle_hedge(
        1_000_000, // initial_shares (entire debt)
        500_000,   // fixed re-borrow
        10_000,    // upfront_fee
        |e| { excess.set(e); Ok::<(), ()>(()) },
        |f| { fee.set(f); Ok::<(), ()>(()) },
    ) {
        Ok(v) => v,
        Err(_) => panic!("settle must not overflow"),
    };

    // Floating value of the closed debt = ceil(1_000_000 × 1_100_000 / 1_000_000).
    assert_eq!(current_value, 1_100_000);
    // Re-borrow into an emptied pool seeds 1:1.
    assert_eq!(new_shares, 500_000);
    // excess = current_value − (borrow_amount + upfront_fee) = 1_100_000 − 510_000.
    assert_eq!(excess.get(), 590_000);
    assert_eq!(fee.get(), 10_000);
    // Position debt rolled: 1_000_000 − 1_000_000 + 500_000.
    assert_eq!(core.position.debt_shares(), 500_000);
    assert_eq!(core.market.total_borrow_assets(), 500_000);
    assert_eq!(core.market.total_borrow_shares(), 500_000);
    // Upfront fee removed from supply.
    assert_eq!(core.market.total_supply_assets(), 1_990_000);
}

/// When the fixed leg exceeds the floating value, the excess saturates to 0
/// (no underflow, no error).
#[test]
fn settle_hedge_excess_saturates_to_zero() {
    use std::cell::Cell;
    let market = TestMarket {
        supply_a: 2_000_000,
        supply_s: 2_000_000,
        borrow_a: 1_100_000,
        borrow_s: 1_000_000,
        ltv: 75,
        ..Default::default()
    };
    let position = TestPosition { collateral: 10_000_000, debt_shares: 1_000_000 };
    let mut core = accrued_core(market, position);

    let excess = Cell::new(u64::MAX);
    if core
        .settle_hedge(
            1_000_000,
            2_000_000, // fixed leg > floating value (1_100_000)
            10_000,
            |e| { excess.set(e); Ok::<(), ()>(()) },
            |_f| Ok::<(), ()>(()),
        )
        .is_err()
    {
        panic!("settle must not overflow");
    }
    assert_eq!(excess.get(), 0, "underwater settlement pays no excess");
}

// ── withdraw_lent (immediate + queued) ────────────────────────────────────

/// Immediate withdrawal burns LP shares and transfers the proportional
/// assets out; both supply totals shrink accordingly. Share price 2.0 here.
#[test]
fn withdraw_lent_immediate_burns_shares_and_transfers_assets() {
    use std::cell::Cell;
    let market = TestMarket {
        supply_a: 2_000_000,
        supply_s: 1_000_000,
        ..Default::default()
    };
    let mut core = Core::new(market);
    let sent = Cell::new(0u64);
    let lend = match core
        .withdraw_lent_immediate(500_000, |amt| { sent.set(amt); Ok::<(), ()>(()) })
    {
        Ok(v) => v,
        Err(_) => panic!("withdraw must succeed"),
    };
    assert_eq!(lend, 1_000_000); // 500_000 × 2_000_000 / 1_000_000
    assert_eq!(sent.get(), 1_000_000);
    assert_eq!(core.market.total_supply_shares(), 500_000);
    assert_eq!(core.market.total_supply_assets(), 1_000_000);
}

/// Withdrawing from an empty pool (no shares) is rejected, not a panic.
#[test]
fn withdraw_lent_immediate_empty_pool_errors() {
    let mut core = Core::new(TestMarket::default());
    assert!(matches!(
        core.withdraw_lent_immediate(1, |_| Ok::<(), ()>(())),
        Err(MathError::InsufficientBalance)
    ));
}

/// Queued withdrawal burns shares and moves the assets into the queue without
/// touching `total_supply_assets` (settled later when the queue is processed).
#[test]
fn withdraw_lent_queued_moves_assets_to_queue() {
    let market = TestMarket {
        supply_a: 2_000_000,
        supply_s: 1_000_000,
        ..Default::default()
    };
    let mut core = Core::new(market);
    let lend = core.withdraw_lent_queued(500_000).expect("queue must succeed");
    assert_eq!(lend, 1_000_000);
    assert_eq!(core.market.total_supply_shares(), 500_000);
    assert_eq!(core.market.assets_in_queue(), 1_000_000);
    assert_eq!(core.market.total_supply_assets(), 2_000_000); // unchanged until processed
}

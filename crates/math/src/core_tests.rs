//! Unit tests for `Core` (kept as a crate submodule via `#[path]` in
//! `core.rs` so they retain access to crate-private items like
//! `SECONDS_PER_YEAR`).

use super::*;
use crate::PRICE_SCALE;

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
    flash: u64,
}
impl Default for TestMarket {
    fn default() -> Self {
        TestMarket {
            supply_a: 0, supply_s: 0, borrow_a: 0, borrow_s: 0,
            last_update: 0, aiq: 0, ltv: 0, fee: 0, fee_shares: 0, flash: 0,
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
    fn flash_loan_outstanding(&self) -> u64 { self.flash }
    fn total_supply_assets_mut(&mut self) -> &mut u64 { &mut self.supply_a }
    fn total_supply_shares_mut(&mut self) -> &mut u64 { &mut self.supply_s }
    fn accrued_fee_shares_mut(&mut self) -> &mut u64 { &mut self.fee_shares }
    fn assets_in_queue_mut(&mut self) -> &mut u64 { &mut self.aiq }
    fn total_borrow_assets_mut(&mut self) -> &mut u64 { &mut self.borrow_a }
    fn total_borrow_shares_mut(&mut self) -> &mut u64 { &mut self.borrow_s }
    fn last_update_mut(&mut self) -> &mut i64 { &mut self.last_update }
    fn flash_loan_outstanding_mut(&mut self) -> &mut u64 { &mut self.flash }
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

/// LP entry/exit lives behind the `Accrued` typestate, so these tests must
/// accrue like the on-chain handlers do. A zero-rate IRM stamped at the
/// market's own `last_update` makes accrual a provable no-op, keeping the
/// share-price arithmetic under test unchanged.
fn accrued(market: TestMarket) -> Core<TestMarket, TestIrm, (), (), Accrued> {
    let irm = TestIrm { rate_bps: 0, current_ts: market.last_update };
    Core::new(market)
        .with_irm(irm)
        .accrue_interest()
        .expect("zero-rate accrual cannot overflow")
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
        flash: 0,
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
        flash: 0,
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

/// `max_borrow_capacity` saturates rather than erroring: at `u64::MAX`
/// collateral and price it clamps to `u64::MAX` instead of overflowing, so a
/// large-but-valid position is never bricked by the capacity gate.
#[test]
fn max_borrow_capacity_refuses_rather_than_clamping_high() {
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
        flash: 0,
    };
    let position = TestPosition { collateral: 0, debt_shares: 0 };
    let core = Core::new(market).with_position(position);
    assert_eq!(
        core.max_borrow_capacity(u64::MAX, u64::MAX),
        None,
        "an unrepresentable capacity must refuse, not clamp high — clamping \
         granted unlimited borrow headroom and released collateral it should \
         have held"
    );
    // A normal case still computes exactly: 1_000_000 × 1.0 × 75% = 750_000.
    assert_eq!(
        core.max_borrow_capacity(1_000_000, PRICE_SCALE as u64),
        Some(750_000)
    );
    // Just inside the representable range still resolves.
    assert_eq!(core.max_borrow_capacity(u64::MAX, 1), Some(13_835_058_055_281));
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
        flash: 0,
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
    match at_cap.borrow(CAPACITY, u64::MAX, |_| Ok::<(), ()>(())) {
        Ok(shares) => assert_eq!(shares, CAPACITY, "first borrow mints 1:1 shares"),
        Err(_) => panic!("borrow at exact capacity must succeed"),
    }

    // One unit over capacity: rejected by the LTV gate.
    let mut over_cap = fresh();
    assert!(
        matches!(
            over_cap.borrow(CAPACITY + 1, u64::MAX, |_| Ok::<(), ()>(())),
            Err(MathError::Undercollateralized)
        ),
        "borrowing capacity + 1 must be Undercollateralized"
    );
}

// ── shared test helpers ──────────────────────────────────────────────────────

/// Builds an `Accrued` core with a zero-elapsed IRM (no interest) and an
/// at-parity oracle, for the borrow paths that need an LTV gate.
fn accrued_core_with_oracle(
    market: TestMarket,
    position: TestPosition,
) -> Core<TestMarket, TestIrm, TestPosition, TestOracle, Accrued> {
    let irm = TestIrm { rate_bps: 0, current_ts: 0 };
    Core::new(market)
        .with_position(position)
        .with_oracle(TestOracle { price: PRICE_SCALE as u64 })
        .with_irm(irm)
        .accrue_interest()
        .expect("accrue must not overflow")
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
    let mut core = accrued(market);
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
    let mut core = accrued(TestMarket::default());
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
    let mut core = accrued(market);
    let lend = core.withdraw_lent_queued(500_000).expect("queue must succeed");
    assert_eq!(lend, 1_000_000);
    assert_eq!(core.market.total_supply_shares(), 500_000);
    assert_eq!(core.market.assets_in_queue(), 1_000_000);
    // Assets leave the supply side with their shares so the price stays 2.0 for
    // whoever is left; they sit in the queue until the payout is cranked.
    assert_eq!(core.market.total_supply_assets(), 1_000_000);
}

/// Queue over-commitment regression: three equal lenders queueing one after
/// another must each be quoted their own deposit back. Debiting shares without
/// their assets left the share price climbing (1x, 1.5x, 3x), promising 550_000
/// against a 300_000 pool so the last in line could never be paid.
#[test]
fn successive_queued_exits_are_priced_flat() {
    let market = TestMarket { supply_a: 300_000, supply_s: 300_000, ..Default::default() };
    let mut core = accrued(market);

    let a = core.withdraw_lent_queued(100_000).expect("a queues");
    let b = core.withdraw_lent_queued(100_000).expect("b queues");
    let c = core.withdraw_lent_queued(100_000).expect("c queues");

    assert_eq!((a, b, c), (100_000, 100_000, 100_000), "share price must not drift");
    assert_eq!(a + b + c, 300_000, "claims must not exceed pool assets");
    assert_eq!(core.market.assets_in_queue(), 300_000);
    assert_eq!(core.market.total_supply_assets(), 0);
    assert_eq!(core.market.total_supply_shares(), 0);
}

// ── process_queued_withdrawal ────────────────────────────────────────────────

/// Enqueue-then-process must land in exactly the same place as an immediate
/// withdrawal: shares burned, assets debited, queue released to zero.
#[test]
fn queued_then_processed_matches_immediate_withdrawal() {
    use std::cell::Cell;
    let market = TestMarket { supply_a: 2_000_000, supply_s: 1_000_000, ..Default::default() };

    let mut immediate = accrued(market);
    let sent_now = Cell::new(0u64);
    immediate
        .withdraw_lent_immediate(500_000, |amt| { sent_now.set(amt); Ok::<(), ()>(()) })
        .unwrap_or_else(|_| panic!("immediate withdrawal must succeed"));

    let mut queued = accrued(market);
    let lend = queued.withdraw_lent_queued(500_000).expect("queue must succeed");
    let sent_later = Cell::new(0u64);
    queued
        .process_queued_withdrawal(lend, |amt| { sent_later.set(amt); Ok::<(), ()>(()) })
        .unwrap_or_else(|_| panic!("processing must succeed"));

    assert_eq!(sent_later.get(), sent_now.get(), "same tokens paid out");
    assert_eq!(queued.market.total_supply_assets(), immediate.market.total_supply_assets());
    assert_eq!(queued.market.total_supply_shares(), immediate.market.total_supply_shares());
    assert_eq!(queued.market.assets_in_queue(), 0, "queue fully released");
}

/// Processing more than is actually queued must error rather than wrap the
/// counters around — the guard against a malformed or double-processed entry.
#[test]
fn process_queued_withdrawal_over_queue_errors() {
    let market = TestMarket { supply_a: 1_000_000, supply_s: 1_000_000, aiq: 10, ..Default::default() };
    let mut core = accrued(market);
    assert!(matches!(
        core.process_queued_withdrawal(11, |_| Ok::<(), ()>(())),
        Err(MathError::Arithmetic)
    ));
}

/// Zero is a no-op, not an error (degenerate band).
#[test]
fn process_queued_withdrawal_zero_is_noop() {
    let market = TestMarket { supply_a: 1_000_000, supply_s: 1_000_000, ..Default::default() };
    let mut core = accrued(market);
    assert!(core.process_queued_withdrawal(0, |_| Ok::<(), ()>(())).is_ok());
    assert_eq!(core.market.total_supply_assets(), 1_000_000);
}

// ── lend_to_collateral_amount (H5) ───────────────────────────────────────────

#[test]
fn lend_to_collateral_inverts_borrow_capacity_conversion() {
    // At price 2.0 (2_000_000 scaled), 1 collateral is worth 2 lend, so a
    // 1_000_000 lend obligation costs 500_000 collateral.
    assert_eq!(crate::lend_to_collateral_amount(1_000_000, 2_000_000), Some(500_000));
    // At parity it is one-for-one.
    assert_eq!(crate::lend_to_collateral_amount(1_000_000, 1_000_000), Some(1_000_000));
}

#[test]
fn lend_to_collateral_rounds_up_and_handles_edges() {
    // 1 lend at price 2.0 = 0.5 collateral → rounds up to 1, favouring the pool.
    assert_eq!(crate::lend_to_collateral_amount(1, 2_000_000), Some(1));
    assert_eq!(crate::lend_to_collateral_amount(0, 2_000_000), Some(0));
    // No price means no honest conversion.
    assert_eq!(crate::lend_to_collateral_amount(1, 0), None);
    // Overflows u64 → None rather than a wrapped amount.
    assert_eq!(crate::lend_to_collateral_amount(u64::MAX, 1), None);
    // Large-but-valid band: 100M tokens at parity.
    const HUNDRED_M: u64 = 100_000_000 * 1_000_000;
    assert_eq!(crate::lend_to_collateral_amount(HUNDRED_M, 1_000_000), Some(HUNDRED_M));
}

// ── flash loans (now owned by Core) ──────────────────────────────────────────

fn flash_market() -> TestMarket {
    TestMarket { supply_a: 1_000_000, supply_s: 1_000_000, ..Default::default() }
}

/// A well-formed loan takes the lock, hands out the principal, and on repay
/// credits only the surplus to lenders — the principal was never theirs to lose.
#[test]
fn flash_loan_round_trip_credits_only_the_fee() {
    use std::cell::Cell;
    let mut core = Core::new(flash_market());
    let out = Cell::new(0u64);

    let min_repay = core
        .flash_borrow(400_000, 1_000_000, |amt| { out.set(amt); Ok::<(), ()>(()) })
        .unwrap_or_else(|_| panic!("borrow must succeed"));

    assert_eq!(out.get(), 400_000);
    assert_eq!(min_repay, 400_000 + crate::flash_fee(400_000).unwrap());
    assert!(core.flash_loan_in_progress());
    // Lenders' assets are untouched mid-loan, so the share price cannot be moved.
    assert_eq!(core.market.total_supply_assets(), 1_000_000);

    let paid = Cell::new(0u64);
    let (principal, fee) = core
        .flash_repay(min_repay, |amt| { paid.set(amt); Ok::<(), ()>(()) })
        .unwrap_or_else(|_| panic!("repay must succeed"));

    assert_eq!((principal, fee), (400_000, crate::flash_fee(400_000).unwrap()));
    assert_eq!(paid.get(), min_repay);
    assert!(!core.flash_loan_in_progress());
    assert_eq!(core.market.total_supply_assets(), 1_000_000 + fee);
}

/// The pairing lock: a second borrow cannot open while one is outstanding.
/// This is what stops N borrows settling against a single repay.
#[test]
fn second_flash_borrow_is_rejected_while_one_is_open() {
    let mut core = Core::new(flash_market());
    core.flash_borrow(400_000, 1_000_000, |_| Ok::<(), ()>(()))
        .unwrap_or_else(|_| panic!("first borrow must succeed"));

    assert!(matches!(
        core.flash_borrow(400_000, 600_000, |_| Ok::<(), ()>(())),
        Err(MathError::FlashLoanOutstanding)
    ));
}

#[test]
fn flash_repay_without_a_loan_is_rejected() {
    let mut core = Core::new(flash_market());
    assert!(matches!(
        core.flash_repay(400_000, |_| Ok::<(), ()>(())),
        Err(MathError::NoFlashLoan)
    ));
}

#[test]
fn flash_repay_must_cover_principal_plus_fee() {
    let mut core = Core::new(flash_market());
    let min_repay = core
        .flash_borrow(400_000, 1_000_000, |_| Ok::<(), ()>(()))
        .unwrap_or_else(|_| panic!("borrow must succeed"));

    assert!(matches!(
        core.flash_repay(min_repay - 1, |_| Ok::<(), ()>(())),
        Err(MathError::FlashLoanUnderRepaid)
    ));
    // The lock survives the failed attempt — the loan is still open.
    assert!(core.flash_loan_in_progress());
}

#[test]
fn flash_borrow_rejects_zero_and_over_liquidity() {
    let mut core = Core::new(flash_market());
    assert!(matches!(
        core.flash_borrow(0, 1_000_000, |_| Ok::<(), ()>(())),
        Err(MathError::InvalidAmount)
    ));
    assert!(matches!(
        core.flash_borrow(1_000_001, 1_000_000, |_| Ok::<(), ()>(())),
        Err(MathError::InsufficientLiquidity)
    ));
}

/// Queued withdrawals are reserved, not lendable — a flash loan may not take
/// them any more than a borrow may.
#[test]
fn flash_borrow_cannot_take_assets_reserved_for_the_queue() {
    let market = TestMarket { supply_a: 400_000, supply_s: 400_000, aiq: 600_000, ..Default::default() };
    let mut core = Core::new(market);
    // Vault physically holds 1_000_000 but 600_000 belongs to the queue.
    assert!(matches!(
        core.flash_borrow(500_000, 1_000_000, |_| Ok::<(), ()>(())),
        Err(MathError::InsufficientLiquidity)
    ));
    assert!(core.flash_borrow(400_000, 1_000_000, |_| Ok::<(), ()>(())).is_ok());
}

// ── borrow liquidity gate (now owned by Core) ────────────────────────────────

#[test]
fn borrow_excludes_queued_assets_from_available_liquidity() {
    let market = TestMarket {
        supply_a: 1_000_000, supply_s: 1_000_000,
        aiq: 600_000, ltv: 75,
        ..Default::default()
    };
    let position = TestPosition { collateral: 10_000_000, debt_shares: 0 };
    let mut core = accrued_core_with_oracle(market, position);

    // Vault holds 1_000_000; 600_000 is spoken for, so only 400_000 is lendable.
    assert!(matches!(
        core.borrow(400_001, 1_000_000, |_| Ok::<(), ()>(())),
        Err(MathError::InsufficientLiquidity)
    ));
    assert!(core.borrow(400_000, 1_000_000, |_| Ok::<(), ()>(())).is_ok());
}

#[test]
fn borrow_rejects_a_zero_amount() {
    let market = TestMarket {
        supply_a: 1_000_000, supply_s: 1_000_000, ltv: 75, ..Default::default()
    };
    let position = TestPosition { collateral: 10_000_000, debt_shares: 0 };
    let mut core = accrued_core_with_oracle(market, position);
    assert!(matches!(
        core.borrow(0, 1_000_000, |_| Ok::<(), ()>(())),
        Err(MathError::InvalidAmount)
    ));
}

// ── configuration predicates ─────────────────────────────────────────────────

#[test]
fn ltv_and_fee_predicates_bound_their_ranges() {
    assert!(!crate::is_valid_ltv_percent(0));
    assert!(crate::is_valid_ltv_percent(1));
    assert!(crate::is_valid_ltv_percent(crate::MAX_LTV_PERCENT));
    // 100 lets a borrower draw their collateral's full value and walk away.
    assert!(!crate::is_valid_ltv_percent(100));
    assert!(!crate::is_valid_ltv_percent(u8::MAX));
}

// ── Client risk figures vs. the on-chain gate (finding 4) ────────────────────

/// The LTV and health factor the client shows must agree with the gate the
/// program actually enforces, at any price — not just at parity.
///
/// A position borrowed to exactly `max_borrow_capacity` sits on the edge of
/// rejection, so by construction it is at HF 1.0 and at the market's configured
/// LTV. While `compute_ltv` / `compute_health_factor` divided raw collateral
/// units by lend units they skipped the oracle entirely, and this position —
/// one unit from being refused — reported HF 2.0 and LTV 37.5%.
#[test]
fn risk_figures_match_the_borrow_gate_at_a_non_parity_price() {
    // Collateral worth half a lend token apiece.
    const HALF: u64 = PRICE_SCALE as u64 / 2;

    let market = TestMarket {
        supply_a: 10_000_000,
        supply_s: 10_000_000,
        ltv: 75,
        ..Default::default()
    };
    let position = TestPosition { collateral: 1_000_000, debt_shares: 0 };
    let irm = TestIrm { rate_bps: 0, current_ts: 0 };
    let fresh = || {
        Core::new(market)
            .with_position(position)
            .with_oracle(TestOracle { price: HALF })
            .with_irm(irm)
            .accrue_interest()
            .expect("accrue must not overflow")
    };

    // 1_000_000 collateral × 0.5 × 75% = 375_000 lend units of capacity.
    let capacity = fresh()
        .max_borrow_capacity(position.collateral, HALF)
        .expect("capacity is representable");
    assert_eq!(capacity, 375_000);

    let mut at_limit = fresh();
    at_limit
        .borrow(capacity, u64::MAX, |_| Ok::<(), ()>(()))
        .unwrap_or_else(|_| panic!("borrowing exactly the capacity must succeed"));
    let debt = at_limit.market.total_borrow_assets();

    assert_eq!(
        crate::compute_ltv(debt, position.collateral, HALF),
        Some(7_500),
        "a position at capacity is at the market's configured LTV"
    );
    assert_eq!(
        crate::compute_health_factor(position.collateral, 75, debt, HALF),
        Some(10_000),
        "a position at capacity is at HF 1.0"
    );

    // And the gate really is one unit away.
    let mut over = fresh();
    assert!(
        matches!(
            over.borrow(capacity + 1, u64::MAX, |_| Ok::<(), ()>(())),
            Err(MathError::Undercollateralized)
        ),
        "one unit past capacity must be rejected"
    );
}

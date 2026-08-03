mod core;
mod traits;

pub use core::*;
pub use traits::*;

const SECONDS_PER_YEAR: u64 = 31_557_600;
pub const PRICE_SCALE: u128 = 1_000_000;

/// Maximum borrow rate the interest math will honor: 10_000% APR
/// (1_000_000 bps). A misconfigured or malicious IRM reporting a higher rate is
/// clamped to this, so interest accrual can never overflow to `None` and brick
/// the pool. The ceiling is far above any plausible crisis rate.
pub const MAX_RATE_BPS: u32 = 1_000_000;

pub enum MathError<E> {
    /// Integer overflow or underflow in a checked arithmetic operation.
    Arithmetic,
    /// Borrow or withdraw would violate the LTV limit.
    Undercollateralized,
    /// Withdrawal exceeds the available balance (collateral deposited or pool shares).
    InsufficientBalance,
    /// The requested amount is too small to mint even one share.
    AmountTooSmall,
    /// A zero (or otherwise meaningless) amount was supplied.
    InvalidAmount,
    /// The vault cannot cover the request once assets reserved for the
    /// withdrawal queue are excluded.
    InsufficientLiquidity,
    /// A flash loan is already in flight on this market.
    FlashLoanOutstanding,
    /// No flash loan is in flight to repay.
    NoFlashLoan,
    /// The repayment does not cover the outstanding principal plus its fee.
    FlashLoanUnderRepaid,
    /// The repayment exceeds the outstanding principal plus its fee. Anything
    /// above that figure would be an unaccounted credit to the supply side; see
    /// [`Core::flash_repay`](crate::Core::flash_repay).
    FlashLoanOverRepaid,
    Transfer(E),
}

/// Smallest deposit that may seed a pool's supply side, in base units.
///
/// Defence in depth against share-price manipulation, in the spirit of Uniswap
/// V2's `MINIMUM_LIQUIDITY`. The first depositor mints shares 1:1, so seeding a
/// pool with a single base unit leaves one share outstanding and makes the
/// share price trivially cheap to move: raise it above a later depositor's
/// whole deposit and their `amount × shares / assets` floors to one share,
/// handing the seeder a cut of it.
///
/// The channel that made that practical — an unbounded surplus credit in
/// [`Core::flash_repay`](crate::Core::flash_repay) — is closed, so this is no
/// longer the load-bearing guard. It stays because the guard should not depend
/// on *no* future instruction ever crediting `total_supply_assets` again:
/// whatever the channel, moving the price now means moving it against at least
/// this many shares, which raises the capital required by the same factor.
///
/// Chosen in base units rather than whole tokens so it does not assume a decimal
/// count. At 6 decimals this is 0.001 of a token — far below any real seeding
/// deposit, and far above the one-unit case the attack needs.
pub const MIN_SEED_LIQUIDITY: u64 = 1_000;

/// Maximum loan-to-value a market may be configured with, in percent.
///
/// Enforces the hard solvency invariant only: at 100 or above a borrower can
/// draw at least what their collateral is worth and walk away — the debt is
/// unsecured on arrival, no price move required. Everything below is left to the
/// market creator as risk appetite (the leverage products depend on very high
/// LTVs). A market near this ceiling has almost no margin before debt exceeds
/// collateral, and until a liquidation path exists nothing can close such a
/// position.
pub const MAX_LTV_PERCENT: u8 = 99;

/// `true` iff `ltv_percent` is a usable market configuration.
pub fn is_valid_ltv_percent(ltv_percent: u8) -> bool {
    ltv_percent > 0 && ltv_percent <= MAX_LTV_PERCENT
}

/// Lend tokens actually available to borrow from a vault holding
/// `vault_balance`.
///
/// Assets reserved for queued withdrawals are excluded: those lenders have
/// already burned their LP and are owed exactly those tokens, so lending them
/// out again would leave the queue unpayable.
pub fn borrowable_liquidity(vault_balance: u64, assets_in_queue: u64) -> u64 {
    vault_balance.saturating_sub(assets_in_queue)
}

/// The same figure as [`borrowable_liquidity`], derived from the market's own
/// accounting instead of the vault's token balance.
///
/// For callers that hold a `Pool` but not the vault token account — the client
/// bindings, above all, which decode account data and never see an SPL balance.
/// Without this they hand-rolled a formula, and it did not agree with the gate:
/// it subtracted `assets_in_queue` from `total_supply_assets − total_borrow_assets`,
/// counting the queue twice and understating what a borrower could actually
/// draw.
///
/// # Why the two agree
///
/// The vault holds what lenders put in, plus what is reserved for the queue,
/// less what has been lent out:
///
/// ```text
/// vault_balance = total_supply_assets + assets_in_queue - total_borrow_assets
/// ```
///
/// Every operation preserves it. `deposit_lent` and `withdraw_lent_immediate`
/// move the vault and `total_supply_assets` together; `borrow` and `repay` move
/// the vault against `total_borrow_assets`; `withdraw_lent_queued` moves
/// `total_supply_assets` into `assets_in_queue` and leaves the vault alone;
/// `process_queued_withdrawal` releases both. Accrual is the one that looks like
/// it should break the identity and does not — it adds the same interest to
/// `total_supply_assets` and `total_borrow_assets`, so their difference, and the
/// vault, are unchanged.
///
/// Substituting gives `vault_balance - assets_in_queue = total_supply_assets -
/// total_borrow_assets`, which is this function. `liquidity_identity_holds_through_a_full_cycle`
/// asserts it against a live `Core` rather than leaving it as an argument on paper.
///
/// A flash loan in flight is the one moment the identity does not hold — the
/// tokens are out of the vault with nothing recorded against them — which is
/// exactly why the operations that price shares refuse to run while
/// `flash_loan_outstanding` is non-zero.
pub fn borrowable_liquidity_from_market(
    total_supply_assets: u64,
    total_borrow_assets: u64,
) -> u64 {
    total_supply_assets.saturating_sub(total_borrow_assets)
}

/// Share of the pool's assets that is unavailable to borrowers, in bps.
///
/// `assets_in_queue` appears on both sides: queued assets no longer belong to
/// current lenders (they left `total_supply_assets` when the exit was queued) but
/// are still pool assets until paid out, and they are just as unavailable to a
/// new borrower as lent-out assets are. Counting them only as "borrowed" without
/// adding them back to the base would overstate utilization as the queue grows.
pub fn utilization_bps(
    total_supply_assets: u64,
    total_borrow_assets: u64,
    assets_in_queue: u64,
) -> u64 {
    let total_assets = total_supply_assets.saturating_add(assets_in_queue);
    if total_assets == 0 {
        return 0;
    }
    let effective_borrowed = total_borrow_assets.saturating_add(assets_in_queue);
    // `effective_borrowed as u128 * 10_000` cannot overflow u128 (operand ≤ u64::MAX),
    // so no masking is needed. If utilization exceeds u64 (only when supply is a
    // tiny fraction of borrowed), saturate high — never report 0, which would
    // falsely signal an idle pool.
    let util = (effective_borrowed as u128) * 10_000 / (total_assets as u128);
    u64::try_from(util).unwrap_or(u64::MAX)
}

pub fn compute_interest(total_borrowed: u64, rate_bps: u32, elapsed_secs: u64) -> Option<u64> {
    if elapsed_secs == 0 || rate_bps == 0 || total_borrowed == 0 {
        return Some(0);
    }
    let rate_bps = rate_bps.min(MAX_RATE_BPS);
    let numerator = (total_borrowed as u128)
        .checked_mul(rate_bps as u128)?
        .checked_mul(elapsed_secs as u128)?;
    let denominator = 10_000u128.checked_mul(SECONDS_PER_YEAR as u128)?;
    let interest = numerator.div_ceil(denominator);
    u64::try_from(interest).ok()
}

/// Call this BEFORE adding `amount` to `total_borrowed`.
///
/// Debt shares are minted with **ceiling** division so a borrower always owes at
/// least their proportional share — rounding favors the protocol, never the
/// borrower. Pairs with the floor rounding in [`amount_to_shares_burned`].
pub fn amount_to_shares(amount: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 && total_borrowed == 0 {
        // Fresh pool: seed debt shares 1:1 with the first borrow.
        return Some(amount);
    }
    if total_debt_shares == 0 || total_borrowed == 0 {
        // Exactly one side is zero — an inconsistent pool (orphan shares with no
        // borrowed assets, or vice versa). Refuse rather than mis-seed 1:1.
        return None;
    }
    let shares = (amount as u128)
        .checked_mul(total_debt_shares as u128)?
        .div_ceil(total_borrowed as u128);
    u64::try_from(shares).ok()
}

/// Uses ceiling division so the protocol never under-collects.
pub fn shares_to_amount(shares: u64, total_borrowed: u64, total_debt_shares: u64) -> Option<u64> {
    if shares == 0 {
        return Some(0);
    }
    if total_debt_shares == 0 {
        return None;
    }
    let numer = (shares as u128).checked_mul(total_borrowed as u128)?;
    let result = numer.div_ceil(total_debt_shares as u128);
    u64::try_from(result).ok()
}

/// Value of `collateral` collateral-token units expressed in lend-token units,
/// as a `u128` so callers can keep scaling without an intermediate clamp.
///
/// This is *the* collateral→lend conversion in the protocol: every solvency and
/// risk figure — the on-chain borrow gate, the client's LTV, the health factor —
/// resolves to this one expression, so none of them can drift from the others.
/// Rounds **down**, understating collateral value, which is the conservative
/// direction for every one of those uses.
///
/// `oracle_price` is lend raw units per collateral raw unit scaled by
/// [`PRICE_SCALE`]; the feed folds the two mints' decimal exponents into it (see
/// `feed::instructions::get_value`), so no decimal adjustment belongs here. A
/// zero price yields zero value — collateral with no usable price is worth
/// nothing for risk purposes.
fn collateral_value_in_lend(collateral: u64, oracle_price: u64) -> u128 {
    // Both operands are ≤ u64::MAX, so the product cannot exceed u128.
    (collateral as u128).saturating_mul(oracle_price as u128) / PRICE_SCALE
}

/// Value of `collateral` collateral-token units in lend-token units at
/// `oracle_price`, clamped to `u64`.
///
/// Inverse of [`lend_to_collateral_amount`]. Rounds **down**; see
/// [`collateral_value_in_lend`] for the shared conversion.
pub fn collateral_to_lend_amount(collateral: u64, oracle_price: u64) -> u64 {
    u64::try_from(collateral_value_in_lend(collateral, oracle_price)).unwrap_or(u64::MAX)
}

/// Convert a lend-denominated `amount` into collateral units at `oracle_price`.
///
/// Inverse of the `collateral × price / PRICE_SCALE` conversion in
/// [`Core::max_borrow_capacity`](crate::Core::max_borrow_capacity). Rounds
/// **up**, so a party settling a lend-side obligation out of posted collateral
/// always surrenders at least the full value — rounding favors the pool.
///
/// Returns `None` when `oracle_price` is 0 (no usable price, so no honest
/// conversion exists) or the result exceeds `u64`.
pub fn lend_to_collateral_amount(amount: u64, oracle_price: u64) -> Option<u64> {
    if amount == 0 {
        return Some(0);
    }
    if oracle_price == 0 {
        return None;
    }
    let collateral = (amount as u128)
        .checked_mul(PRICE_SCALE)?
        .div_ceil(oracle_price as u128);
    u64::try_from(collateral).ok()
}

/// Loan-to-value of a position in bps: debt as a fraction of the *lend value* of
/// its collateral at `oracle_price`.
///
/// Debt is denominated in lend units and collateral in collateral units, so the
/// two are only comparable once collateral has crossed the oracle. Dividing them
/// directly silently assumed price parity and equal decimals — it reported the
/// right number only for a 1:1 pair, and understated LTV for exactly the
/// positions most at risk (collateral cheaper than the borrowed asset).
///
/// Returns `None` when there is no debt (nothing to measure), and when the
/// collateral has no lend value — a zero or missing price, where the true LTV is
/// unbounded rather than zero. Reporting `Some(0)` there would paint an
/// unbacked position as perfectly safe; callers should surface "unknown".
pub fn compute_ltv(debt: u64, collateral: u64, oracle_price: u64) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    let collateral_value = collateral_value_in_lend(collateral, oracle_price);
    if collateral_value == 0 {
        return None;
    }
    let ltv_bps = (debt as u128).checked_mul(10_000)? / collateral_value;
    u32::try_from(ltv_bps).ok()
}

/// Health factor of a position in bps, where 10_000 is exactly at the limit.
///
/// Defined as `max_borrow_capacity / debt`, so it crosses 10_000 at precisely
/// the point [`Core::borrow`](crate::Core::borrow) starts rejecting — the client
/// warns on the same boundary the chain enforces. Like [`compute_ltv`] this needs
/// `oracle_price` to value the collateral; without it the figure was only correct
/// for a 1:1 pair and overstated health when collateral was worth less than the
/// borrowed asset.
///
/// Returns `None` when there is no debt. Collateral with no lend value yields
/// `Some(0)` — maximally unhealthy, which is the truthful reading.
pub fn compute_health_factor(
    collateral: u64,
    ltv_percent: u8,
    debt: u64,
    oracle_price: u64,
) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    let capacity = collateral_value_in_lend(collateral, oracle_price)
        .checked_mul(ltv_percent as u128)?
        / 100;
    let hf_bps = capacity.checked_mul(10_000)? / (debt as u128);
    u32::try_from(hf_bps).ok()
}

pub fn compute_liquidation_threshold(debt: u64, collateral: u64, ltv_percent: u8) -> Option<u32> {
    if debt == 0 {
        return None;
    }
    if collateral == 0 || ltv_percent == 0 {
        return Some(0);
    }
    let numerator = (debt as u128).checked_mul(1_000_000)?;
    let denominator = (collateral as u128).checked_mul(ltv_percent as u128)?;
    let liq_bps = numerator / denominator;
    u32::try_from(liq_bps).ok()
}

/// Shares burned when repaying `repay_amount`, capped at `max_shares` so a repay
/// never burns more shares than the position holds.
///
/// Uses **floor** division: a partial repay burns no more shares than the tokens
/// received are worth, so the pool never loses (rounding favors the protocol).
/// Pairs with the ceiling rounding in [`amount_to_shares`]. Returns `None` when
/// `total_borrowed == 0` (no debt to burn against) rather than dividing by zero.
pub fn amount_to_shares_burned(
    repay_amount: u64,
    total_borrowed: u64,
    total_debt_shares: u64,
    max_shares: u64,
) -> Option<u64> {
    if repay_amount == 0 {
        return Some(0);
    }
    if total_borrowed == 0 {
        return None;
    }
    let shares = (repay_amount as u128).checked_mul(total_debt_shares as u128)?;
    let shares = shares / (total_borrowed as u128);
    let shares = u64::try_from(shares).ok()?.min(max_shares);
    Some(shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    const YEAR: u64 = SECONDS_PER_YEAR;
    /// Oracle price at parity: 1 collateral unit is worth 1 lend unit.
    const PARITY: u64 = PRICE_SCALE as u64;

    #[test]
    fn zero_elapsed_is_zero_interest() {
        assert_eq!(compute_interest(1_000_000, 500, 0), Some(0));
    }

    #[test]
    fn zero_principal_is_zero_interest() {
        assert_eq!(compute_interest(0, 500, YEAR), Some(0));
    }

    #[test]
    fn zero_rate_is_zero_interest() {
        assert_eq!(compute_interest(1_000_000, 0, YEAR), Some(0));
    }

    #[test]
    fn one_year_hundred_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 10_000, YEAR), Some(1_000_000));
    }

    #[test]
    fn one_year_fifty_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 5_000, YEAR), Some(500_000));
    }

    #[test]
    fn one_year_one_percent_apr() {
        assert_eq!(compute_interest(1_000_000, 100, YEAR), Some(10_000));
    }

    #[test]
    fn half_year_is_half_of_full_year() {
        let full = compute_interest(1_000_000, 5_000, YEAR).unwrap();
        let half = compute_interest(1_000_000, 5_000, YEAR / 2).unwrap();
        assert!((full / 2).abs_diff(half) <= 1);
    }

    #[test]
    fn graceful_none_on_u64_overflow() {
        // u64::MAX × u32::MAX overflows u128 in the second checked_mul → None.
        assert_eq!(compute_interest(u64::MAX, u32::MAX, 1_000 * YEAR), None);
    }

    #[test]
    fn zero_amount_live_pool_is_zero_shares() {
        // Proves the amount == 0 early-return is taken, not the ratio formula.
        assert_eq!(amount_to_shares(0, 1_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn first_borrow_is_one_to_one() {
        assert_eq!(amount_to_shares(500, 0, 0), Some(500));
    }

    #[test]
    fn round_trip_single_borrower() {
        let amount = 1_000_000u64;
        let shares = amount_to_shares(amount, 0, 0).unwrap();
        assert_eq!(shares, amount);
        let back = shares_to_amount(shares, amount, shares).unwrap();
        assert!(back.abs_diff(amount) <= 1);
    }

    #[test]
    fn second_borrow_after_interest_accrual() {
        // Ceiling division rounds the borrower's debt shares up: 90_909.09 → 90_910.
        let shares = amount_to_shares(100_000, 1_100_000, 1_000_000).unwrap();
        assert_eq!(shares, 90_910);
        let new_total_borrowed = 1_100_000 + 100_000;
        let new_total_shares = 1_000_000 + shares;
        let second_debt = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(second_debt.abs_diff(100_000) <= 1);
    }

    #[test]
    fn zero_shares_is_zero_amount() {
        assert_eq!(shares_to_amount(0, 1_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn nonzero_shares_zero_total_shares_returns_none() {
        // Inconsistent state (e.g. stale pool data during a race condition) must not panic.
        assert_eq!(shares_to_amount(1, 1_000_000, 0), None);
    }

    // ── Group A: Minimal amounts (1 base unit) ────────────────────────────────

    #[test]
    fn minimal_first_borrow_one_unit() {
        assert_eq!(amount_to_shares(1, 0, 0), Some(1));
    }

    #[test]
    fn minimal_shares_to_amount_one_unit() {
        assert_eq!(shares_to_amount(1, 1, 1), Some(1));
    }

    #[test]
    fn minimal_repay_burns_one_share() {
        assert_eq!(amount_to_shares_burned(1, 1, 1, 1), Some(1));
    }

    #[test]
    fn minimal_repay_one_unit_from_large_pool() {
        // Ceiling division: 1 unit repaid burns exactly 1 share regardless of pool size.
        const HUNDRED_M: u64 = 100_000_000 * 1_000_000;
        assert_eq!(
            amount_to_shares_burned(1, HUNDRED_M, HUNDRED_M, HUNDRED_M),
            Some(1)
        );
    }

    #[test]
    fn minimal_interest_one_year_100pct() {
        // ceil(1 × 10_000 × YEAR / (10_000 × YEAR)) = ceil(1.0) = 1
        assert_eq!(compute_interest(1, 10_000, YEAR), Some(1));
    }

    #[test]
    fn minimal_interest_one_year_1pct() {
        // ceil(1 × 100 × YEAR / (10_000 × YEAR)) = ceil(0.01) = 1 (rounds up)
        assert_eq!(compute_interest(1, 100, YEAR), Some(1));
    }

    #[test]
    fn minimal_interest_one_second() {
        // ceil(1 × 10_000 × 1 / (10_000 × YEAR)) = ceil(1/YEAR) = 1 (rounds up)
        assert_eq!(compute_interest(1, 10_000, 1), Some(1));
    }

    // ── Group B: Huge amounts (100M tokens = 10^14 base units) ───────────────

    const HUNDRED_M: u64 = 100_000_000 * 1_000_000; // 10^14 base units
    const SEVENTY_FIVE_M: u64 = 75_000_000 * 1_000_000;
    const FIFTY_M: u64 = 50_000_000 * 1_000_000;

    #[test]
    fn huge_first_borrow_100m() {
        assert_eq!(amount_to_shares(HUNDRED_M, 0, 0), Some(HUNDRED_M));
    }

    #[test]
    fn huge_round_trip_single_borrower() {
        let shares = amount_to_shares(HUNDRED_M, 0, 0).unwrap();
        assert_eq!(shares, HUNDRED_M);
        let back = shares_to_amount(shares, HUNDRED_M, shares).unwrap();
        assert_eq!(back, HUNDRED_M);
    }

    #[test]
    fn huge_second_borrow_after_10pct_interest() {
        // Pool has 110M assets and 100M shares after 10% interest accrual.
        // New 100M borrow gets ceil(100M × 100M / 110M) = 90_909_090_909_091 shares
        // (rounded up so the borrower never under-owes).
        let total_after_interest = 110_000_000u64 * 1_000_000;
        let shares = amount_to_shares(HUNDRED_M, total_after_interest, HUNDRED_M).unwrap();
        assert_eq!(shares, 90_909_090_909_091);

        // Ceiling on repay brings it back within 1 unit of the original borrow.
        let new_total_borrowed = total_after_interest + HUNDRED_M;
        let new_total_shares = HUNDRED_M + shares;
        let back = shares_to_amount(shares, new_total_borrowed, new_total_shares).unwrap();
        assert!(back.abs_diff(HUNDRED_M) <= 1);
    }

    #[test]
    fn huge_interest_10pct_one_year() {
        let interest = compute_interest(HUNDRED_M, 1_000, YEAR).unwrap();
        assert_eq!(interest, 10_000_000 * 1_000_000);
    }

    #[test]
    fn huge_interest_100pct_one_year() {
        assert_eq!(compute_interest(HUNDRED_M, 10_000, YEAR), Some(HUNDRED_M));
    }

    #[test]
    fn huge_flash_fee() {
        // 9 bps of 100M tokens = 90_000 tokens = 90_000_000_000 base units.
        assert_eq!(flash_fee(HUNDRED_M), Some(90_000_000_000));
    }

    #[test]
    fn huge_compute_ltv_at_75pct() {
        // 75M debt / 100M collateral = 7500 bps.
        assert_eq!(compute_ltv(SEVENTY_FIVE_M, HUNDRED_M, PARITY), Some(7_500));
    }

    #[test]
    fn huge_health_factor_at_liquidation_limit() {
        // collateral=100M, ltv=75%, debt=75M → HF = 100M×75×100/75M = 10_000 (exactly at limit).
        assert_eq!(
            compute_health_factor(HUNDRED_M, 75, SEVENTY_FIVE_M, PARITY),
            Some(10_000)
        );
    }

    #[test]
    fn huge_health_factor_healthy() {
        // collateral=100M, ltv=75%, debt=50M → HF = 100M×75×100/50M = 15_000.
        assert_eq!(
            compute_health_factor(HUNDRED_M, 75, FIFTY_M, PARITY),
            Some(15_000)
        );
    }

    #[test]
    fn huge_full_repay() {
        // Repaying the full 100M against a 100M pool burns all 100M shares.
        assert_eq!(
            amount_to_shares_burned(HUNDRED_M, HUNDRED_M, HUNDRED_M, HUNDRED_M),
            Some(HUNDRED_M)
        );
    }

    // ── Group C: Near-u64::MAX — must succeed ─────────────────────────────────

    #[test]
    fn interest_exact_u64_max_boundary() {
        // numerator = u64::MAX × 10_000 × YEAR; denominator = 10_000 × YEAR.
        // interest = ceil(u64::MAX) = u64::MAX — fits exactly in u64.
        assert_eq!(compute_interest(u64::MAX, 10_000, YEAR), Some(u64::MAX));
    }

    #[test]
    fn interest_u64_max_tiny_rate_one_sec() {
        // u64::MAX × 500 / (10_000 × YEAR) ≈ 29 billion — fits well in u64.
        let v = compute_interest(u64::MAX, 500, 1).unwrap();
        assert!(v > 0 && v < u64::MAX);
    }

    #[test]
    fn flash_fee_u64_max_succeeds() {
        // 9 × u64::MAX fits in u128; ceil(… / 10_000) fits back in u64.
        let expected = u64::try_from(
            (u64::MAX as u128).checked_mul(9).unwrap().div_ceil(10_000)
        ).unwrap();
        assert_eq!(flash_fee(u64::MAX), Some(expected));
    }

    // ── Group D: Must return None — no panic, no incorrect result ─────────────

    #[test]
    fn interest_one_second_over_u64_max_returns_none() {
        // YEAR+1 seconds: interest = u64::MAX + ~584M > u64::MAX → try_from fails.
        assert_eq!(compute_interest(u64::MAX, 10_000, YEAR + 1), None);
    }

    #[test]
    fn amount_to_shares_result_overflows_u64_returns_none() {
        // u64::MAX × u64::MAX fits u128 but the quotient won't fit u64.
        assert_eq!(amount_to_shares(u64::MAX, 1, u64::MAX), None);
    }

    #[test]
    fn shares_to_amount_result_overflows_u64_returns_none() {
        assert_eq!(shares_to_amount(u64::MAX, u64::MAX, 1), None);
    }

    #[test]
    fn amount_to_shares_burned_overflow_returns_none() {
        assert_eq!(
            amount_to_shares_burned(u64::MAX, 1, u64::MAX, u64::MAX),
            None
        );
    }

    #[test]
    fn health_factor_overflows_u32_returns_none() {
        // collateral=u64::MAX, ltv=100, debt=1 → HF ≫ u32::MAX → None.
        assert_eq!(compute_health_factor(u64::MAX, 100, 1, PARITY), None);
    }

    #[test]
    fn health_factor_zero_debt_returns_none() {
        // debt == 0 is the early-return guard; removing it would cause a divide-by-zero panic.
        assert_eq!(compute_health_factor(1_000_000, 75, 0, PARITY), None);
    }

    // ── compute_ltv missing None test ─────────────────────────────────────────

    #[test]
    fn compute_ltv_overflows_u32_returns_none() {
        // ltv_bps = u64::MAX × 10_000 / 1 ≈ 1.84×10²³ >> u32::MAX → try_from fails.
        assert_eq!(compute_ltv(u64::MAX, 1, PARITY), None);
    }

    // ── compute_liquidation_threshold — full four-band coverage ───────────────

    #[test]
    fn liq_threshold_zero_debt_returns_none() {
        assert_eq!(compute_liquidation_threshold(0, 1_000_000, 75), None);
    }

    #[test]
    fn liq_threshold_zero_collateral_returns_zero() {
        assert_eq!(compute_liquidation_threshold(1_000_000, 0, 75), Some(0));
    }

    #[test]
    fn liq_threshold_zero_ltv_returns_zero() {
        assert_eq!(compute_liquidation_threshold(1_000_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn liq_threshold_at_exact_ltv_returns_price_scale() {
        // debt=75M, collateral=100M, ltv=75 → liq_price = 75M×1_000_000 / (100M×75) = 10_000.
        assert_eq!(
            compute_liquidation_threshold(SEVENTY_FIVE_M, HUNDRED_M, 75),
            Some(10_000)
        );
    }

    #[test]
    fn liq_threshold_overflows_u32_returns_none() {
        // liq_bps = u64::MAX × 1_000_000 / (1 × 1) >> u32::MAX → None.
        assert_eq!(compute_liquidation_threshold(u64::MAX, 1, 1), None);
    }

    // ── Rounding direction: shares must always favor the protocol ─────────────

    #[test]
    fn amount_to_shares_burned_zero_borrowed_returns_none() {
        // total_borrowed == 0 used to divide by zero (panic). Must return None.
        assert_eq!(amount_to_shares_burned(1, 0, 1_000_000, 1_000_000), None);
    }

    #[test]
    fn borrow_shares_round_up() {
        // 1 unit into a pool where 1 share is worth 2 units: exact = 0.5 share.
        // Ceiling mints 1 share so the borrower never owes 0 for a real borrow.
        assert_eq!(amount_to_shares(1, 2, 1), Some(1));
    }

    #[test]
    fn repay_shares_burned_round_down() {
        // Repay 1 unit where 1 share is worth 2 units: exact = 0.5 share.
        // Floor burns 0 shares so the pool never releases more debt than paid for.
        assert_eq!(amount_to_shares_burned(1, 2, 1, 1), Some(0));
    }

    #[test]
    fn borrow_owes_at_least_what_was_borrowed() {
        // Ceiling mint + ceiling valuation guarantee the debt never rounds below
        // the borrowed amount — the invariant that keeps the pool fully backed.
        let borrowed = 100_000u64;
        let (tb, ts) = (1_100_000u64, 1_000_000u64);
        let shares = amount_to_shares(borrowed, tb, ts).unwrap();
        let owed = shares_to_amount(shares, tb + borrowed, ts + shares).unwrap();
        assert!(owed >= borrowed, "owed {owed} < borrowed {borrowed}");
    }

    // ── flash_fee zero and minimal ─────────────────────────────────────────────

    #[test]
    fn flash_fee_zero_amount_is_zero() {
        assert_eq!(flash_fee(0), Some(0));
    }

    #[test]
    fn flash_fee_tiny_amount_charges_minimum_one() {
        // Ceiling + min(1): a nonzero flash loan is never free.
        // 1 × 9 = 9 → ceil(9/10_000) = 1; 1_111 × 9 = 9_999 → ceil = 1.
        assert_eq!(flash_fee(1), Some(1));
        assert_eq!(flash_fee(1_111), Some(1));
    }

    #[test]
    fn flash_fee_rounds_up() {
        // 1_112 × 9 = 10_008 → ceil(10_008 / 10_000) = 2 (was 1 under floor).
        assert_eq!(flash_fee(1_112), Some(2));
    }

    // ── M5: interest-rate ceiling ─────────────────────────────────────────────

    #[test]
    fn interest_rate_above_max_is_clamped() {
        // A rate 10× over the ceiling yields the same interest as the ceiling —
        // accrual never overflows to None, so the pool can't be bricked.
        let at_cap = compute_interest(1_000_000, MAX_RATE_BPS, YEAR);
        let over = compute_interest(1_000_000, MAX_RATE_BPS * 10, YEAR);
        assert_eq!(over, at_cap);
        // 10_000% APR on 1_000_000 principal for one year = 100_000_000.
        assert_eq!(at_cap, Some(100_000_000));
    }

    // ── L2: utilization ───────────────────────────────────────────────────────

    #[test]
    fn utilization_half_and_zero_supply() {
        assert_eq!(utilization_bps(1_000_000, 500_000, 0), 5_000);
        assert_eq!(utilization_bps(0, 500_000, 0), 0);
    }

    #[test]
    fn utilization_counts_queued_assets_on_both_sides() {
        // A pool holding 1_100_000 assets with 400_000 lent out and 100_000
        // reserved for the queue has 500_000 unavailable of 1_100_000.
        assert_eq!(utilization_bps(1_000_000, 400_000, 100_000), 4_545);
        // Everything reserved for the queue and nothing lent: fully utilized.
        assert_eq!(utilization_bps(0, 0, 100_000), 10_000);
        // An idle pool with an empty queue stays at zero.
        assert_eq!(utilization_bps(1_000_000, 0, 0), 0);
    }

    // ── L4: amount_to_shares rejects inconsistent one-sided-zero state ────────

    #[test]
    fn amount_to_shares_orphan_shares_no_borrowed_returns_none() {
        // shares outstanding but zero borrowed assets — inconsistent, reject.
        assert_eq!(amount_to_shares(500, 0, 1_000_000), None);
        // borrowed assets but zero shares — equally inconsistent.
        assert_eq!(amount_to_shares(500, 1_000_000, 0), None);
    }

    // ── collateral_to_lend_amount: the shared oracle conversion ───────────────

    const HALF: u64 = PRICE_SCALE as u64 / 2; // collateral worth 0.5 lend
    const DOUBLE: u64 = PRICE_SCALE as u64 * 2; // collateral worth 2 lend

    #[test]
    fn collateral_value_zero_and_minimal() {
        assert_eq!(collateral_to_lend_amount(0, DOUBLE), 0);
        assert_eq!(collateral_to_lend_amount(1, PARITY), 1);
        // Rounds down: one unit of near-worthless collateral is worth nothing.
        assert_eq!(collateral_to_lend_amount(1, 1), 0);
    }

    #[test]
    fn collateral_value_scales_with_price() {
        assert_eq!(collateral_to_lend_amount(HUNDRED_M, PARITY), HUNDRED_M);
        assert_eq!(collateral_to_lend_amount(HUNDRED_M, HALF), HUNDRED_M / 2);
        assert_eq!(collateral_to_lend_amount(HUNDRED_M, DOUBLE), HUNDRED_M * 2);
        // No price is no value — never treat unpriced collateral as free money.
        assert_eq!(collateral_to_lend_amount(HUNDRED_M, 0), 0);
    }

    #[test]
    fn collateral_value_clamps_high_rather_than_wrapping() {
        // u64::MAX² / PRICE_SCALE ≈ 3.4×10³² — far past u64, so it clamps.
        assert_eq!(collateral_to_lend_amount(u64::MAX, u64::MAX), u64::MAX);
    }

    #[test]
    fn collateral_value_inverts_lend_to_collateral() {
        // The two conversions are inverses; they round opposite ways (down here,
        // up there), so a round trip lands within one unit — never below.
        for price in [HALF, PARITY, DOUBLE] {
            let lend = collateral_to_lend_amount(1_000_000, price);
            let back = lend_to_collateral_amount(lend, price).unwrap();
            assert!(
                back.abs_diff(1_000_000) <= 1,
                "round trip at price {price}: {back} vs 1_000_000"
            );
        }
    }

    // ── LTV / health factor must cross the oracle (finding 4) ────────────────

    #[test]
    fn ltv_measures_debt_against_priced_collateral() {
        // 25 debt against 100 collateral worth 0.5 each = 50 lend → 5_000 bps.
        // Dividing the raw units instead reported 2_500 — half the real LTV.
        assert_eq!(compute_ltv(25, 100, HALF), Some(5_000));
        assert_eq!(compute_ltv(25, 100, PARITY), Some(2_500));
        assert_eq!(compute_ltv(25, 100, DOUBLE), Some(1_250));
    }

    #[test]
    fn ltv_is_unknown_without_a_usable_price() {
        // Debt against collateral of no lend value: the true LTV is unbounded.
        // `Some(0)` would render an unbacked position as perfectly safe.
        assert_eq!(compute_ltv(1_000_000, 1_000_000, 0), None);
        assert_eq!(compute_ltv(1_000_000, 0, PARITY), None);
        // No debt is not a risk figure at all.
        assert_eq!(compute_ltv(0, 1_000_000, PARITY), None);
    }

    #[test]
    fn health_factor_hits_the_limit_where_the_borrow_gate_does() {
        // capacity = 1_000_000 collateral × 0.5 × 75% = 375_000 lend.
        // At exactly that debt the position sits on the gate: HF = 10_000.
        assert_eq!(compute_health_factor(1_000_000, 75, 375_000, HALF), Some(10_000));
        // Half the debt is twice the headroom.
        assert_eq!(compute_health_factor(1_000_000, 75, 187_500, HALF), Some(20_000));
        // Ignoring the price reported 20_000 for the at-limit case above — a
        // position one tick from rejection shown as 2× overcollateralized.
        assert!(
            compute_health_factor(1_000_000, 75, 375_000, PARITY).unwrap() > 10_000,
            "parity must not be the answer for a 0.5-priced pair"
        );
    }

    #[test]
    fn health_factor_of_unpriceable_collateral_is_zero() {
        // Unlike LTV this is well defined: no value backing debt is as unhealthy
        // as a position gets, and 0 is the honest reading.
        assert_eq!(compute_health_factor(1_000_000, 75, 1_000, 0), Some(0));
        assert_eq!(compute_health_factor(0, 75, 1_000, PARITY), Some(0));
    }

    #[test]
    fn health_factor_and_ltv_agree_at_the_configured_ltv() {
        // A position whose LTV reads exactly the market's `ltv_percent` is by
        // definition at HF 1.0, whatever the price. This is the invariant the two
        // functions must satisfy jointly — it failed for every non-parity price
        // while they disagreed about whether to consult the oracle.
        for price in [HALF, PARITY, DOUBLE] {
            let collateral = 1_000_000u64;
            let debt = collateral_to_lend_amount(collateral, price) * 75 / 100;
            assert_eq!(compute_ltv(debt, collateral, price), Some(7_500));
            assert_eq!(
                compute_health_factor(collateral, 75, debt, price),
                Some(10_000)
            );
        }
    }
}

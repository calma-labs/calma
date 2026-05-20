import {
    amount_to_shares,
    amount_to_shares_burned,
    compute_interest,
    shares_to_amount,
} from '@jbl/wasm-lib'

/**
 * Convert debt shares to the current outstanding token amount.
 * Mirrors the on-chain ceiling division — returns `undefined` on overflow.
 */
export function sharesToAmount(
    shares: bigint,
    totalBorrowed: bigint,
    totalDebtShares: bigint,
): bigint | undefined {
    return shares_to_amount(shares, totalBorrowed, totalDebtShares)
}

/**
 * Compute simple interest accrued over `elapsedSecs`.
 * Returns `undefined` on overflow.
 */
export function computeInterest(
    totalBorrowed: bigint,
    rateBps: number,
    elapsedSecs: bigint,
): bigint | undefined {
    return compute_interest(totalBorrowed, rateBps, elapsedSecs)
}

/**
 * Convert a borrow amount to debt shares given the current pool state.
 * Call BEFORE adding `amount` to `totalBorrowed`.
 * Returns `undefined` on overflow.
 */
export function amountToShares(
    amount: bigint,
    totalBorrowed: bigint,
    totalDebtShares: bigint,
): bigint | undefined {
    return amount_to_shares(amount, totalBorrowed, totalDebtShares)
}

/**
 * Convert a repay token amount to the number of debt shares to burn.
 * Capped at `maxShares` to handle full-repay rounding.
 * Returns `undefined` on overflow.
 */
export function amountToSharesBurned(
    repayAmount: bigint,
    totalBorrowed: bigint,
    totalDebtShares: bigint,
    maxShares: bigint,
): bigint | undefined {
    return amount_to_shares_burned(repayAmount, totalBorrowed, totalDebtShares, maxShares)
}

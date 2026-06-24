/**
 * Display-only projections for leveraged ("multiply") positions.
 *
 * These are floating-point UI estimates with no on-chain equivalent — the
 * program never computes "leverage" or "net APY" — so they live in TS rather
 * than the shared `math` crate (which deals only in integer base units / bps).
 * Centralised here so every view uses one definition instead of re-deriving it.
 */

/**
 * Effective leverage of a position: `collateral / equity`, where
 * `equity = collateral − debt`. Returns 1 when equity is non-positive.
 */
export function positionLeverage(collateral: number, debt: number): number {
    const equity = collateral - debt
    if (equity <= 0 || collateral <= 0) return 1
    return collateral / equity
}

/**
 * Net APY of a leveraged position: `L·supplyAPY − (L−1)·borrowAPY`, floored at 0.
 * All APYs are in percent.
 */
export function leveragedNetAPY(
    leverage: number,
    supplyAPY: number,
    borrowAPY: number,
): number {
    return Math.max(0, leverage * supplyAPY - (leverage - 1) * borrowAPY)
}

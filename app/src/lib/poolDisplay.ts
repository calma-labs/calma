import { getPoolMeta } from '@/config/poolRegistry'
import { PoolAccount } from '@jbl/wasm-lib'
import { PublicKey } from '@solana/web3.js'
import type { Pool } from '@/types/pool'


/**
 * On-chain token amounts use 6 decimal places (USDC / USDT style).
 * Divide raw bigint/number values by this factor to get UI amounts.
 */
const LEND_DECIMALS = 6
const COLLATERAL_DECIMALS = 6
const DECIMALS_FACTOR = 10 ** LEND_DECIMALS

/**
 * Derive APY figures and utilization from on-chain pool state.
 * Assumes 6-decimal lend tokens (USDC/USDT style). Supply APY uses the
 * standard formula: supply_apy = borrow_apy × utilization.
 */
export function derivePoolMetrics(pd: PoolAccount): {
    utilizationBps: number
    utilizationPct: number
    borrowAPY: number
    supplyAPY: number
    totalBorrowedRaw: number
    totalLendRaw: number
    availableLiquidityRaw: number
    totalCollateralRaw: number
} {
    const totalLendRaw = Number(pd.total_lend_deposited)
    const totalBorrowedRaw = Number(pd.total_borrowed)
    const totalCollateralRaw = Number(pd.total_collateral_deposited)
    const pendingWithdrawalsRaw = Number(pd.pending_withdrawals())

    // total_lend_deposited tracks total deposits and does NOT decrease when
    // tokens are borrowed out. Utilization mirrors the on-chain formula:
    //   utilization = total_borrowed / total_lend_deposited
    // Pending withdrawals are treated as committed (unavailable) capital, so
    // they are added to the effective borrowed amount for utilization.
    const effectiveBorrowedRaw = totalBorrowedRaw + pendingWithdrawalsRaw
    const utilizationBps =
        totalLendRaw > 0
            ? Math.round((effectiveBorrowedRaw / totalLendRaw) * 10_000)
            : 0

    const utilizationPct = utilizationBps / 100

    const borrowRateBps = pd.fee_bps(utilizationBps)
    const borrowAPY = borrowRateBps / 100
    const baseUtilizationPct = totalLendRaw > 0 ? (totalBorrowedRaw / totalLendRaw) * 100 : 0
    const supplyAPY = borrowAPY * (baseUtilizationPct / 100)

    const availableLiquidityRaw = totalLendRaw - effectiveBorrowedRaw

    return {
        utilizationBps,
        utilizationPct,
        borrowAPY,
        supplyAPY,
        totalBorrowedRaw,
        totalLendRaw,
        availableLiquidityRaw,
        totalCollateralRaw,
    }
}

/**
 * Map on-chain PoolAccount to the display-friendly Pool shape used by UI
 * components. Amounts are kept as raw token counts (no USD valuation since
 * no price oracle is available).
 *
 * `address` and `id` are both the pool's own PublicKey base-58 string so
 * routing with `/pool/:address` resolves back to the same account.
 */
export function poolDataToDisplayPool(publicKey: PublicKey, pd: PoolAccount): Pool {
    const metrics = derivePoolMetrics(pd)
    const addr = publicKey.toBase58()
    const meta = getPoolMeta(addr)

    return {
        id: addr,
        address: addr,
        ...meta,
        supplyAPY: metrics.supplyAPY,
        borrowAPY: metrics.borrowAPY,
        // Convert raw 6-decimal amounts to human-readable UI values
        totalSupplied: metrics.totalLendRaw / DECIMALS_FACTOR,
        totalBorrowed: metrics.totalBorrowedRaw / DECIMALS_FACTOR,
        totalCollateral: metrics.totalCollateralRaw / (10 ** COLLATERAL_DECIMALS),
        utilization: metrics.utilizationPct,
        ltv: pd.ltv_percent,
        availableLiquidity: metrics.availableLiquidityRaw / DECIMALS_FACTOR,
    }
}

import { getTokenMeta } from '@/lib/tokenRegistry'
import { token_amount_to_f64 } from '@calma/wasm-lib'
import { generatePortfolioHistory } from '@/lib/mocks/portfolio.mock'
import type {
    BorrowPosition,
    LendPosition,
    MultiplyPosition,
    PortfolioHistoryPoint,
    PortfolioSummary,
} from '@/types/portfolio'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey } from '@solana/web3.js'
import { useMemo } from 'react'
import { useUserPositionsByAuthority } from './program/useUserPosition'
import { useValidLendingAccounts } from './program/useValidLendingAccounts'
import { useMintDecimalsMap } from './useMintDecimals'
import { useWalletBalances } from './useWalletBalances'

// ─── Internal helpers ─────────────────────────────────────────────────────────

function useWalletPublicKey(): PublicKey | null {
    const { connected, wallet } = useWalletConnection()
    return useMemo(() => {
        if (!connected || !wallet) return null
        return new PublicKey(wallet.account.publicKey)
    }, [connected, wallet])
}

// ─── Lend Positions ────────────────────────────────────────────────────────────
// Derived from LP token balances held in the connected wallet.
// LP share → proportional claim on total lend supply.

export function useLendPositions(enabled = true) {
    const { data: pools = [], isLoading: poolsLoading } = useValidLendingAccounts()
    const { data: balances, isLoading: balancesLoading } = useWalletBalances()

    const lendMints = useMemo(() => pools.map((p) => new PublicKey(p.account.lend_mint)), [pools])
    const decimalsMap = useMintDecimalsMap(lendMints)

    const data = useMemo<LendPosition[]>(() => {
        if (!enabled || !pools.length || !balances?.tokens.length) return []

        return pools.flatMap((pool) => {
            const lpToken = balances.tokens.find((t) => t.mint.equals(new PublicKey(pool.account.lp_mint)))
            if (!lpToken || lpToken.amount === 0n) return []

            const suppliedRaw = pool.account.lend_for_shares(lpToken.amount)
            if (suppliedRaw == null) return []
            const lendDecimals = decimalsMap.get(new PublicKey(pool.account.lend_mint).toBase58()) ?? 6
            const supplied = token_amount_to_f64(suppliedRaw, lendDecimals)

            // Health proxy: how easy it is to withdraw — decreases with utilization.
            // 100 = fully liquid pool, 0 = fully utilized (no liquidity to withdraw).
            const health = Math.max(0, Math.min(100, Math.round(100 - pool.account.utilization_bps() / 100)))

            // Rough earned estimate (~1 month at current APY). No historical data on-chain.
            const earnedEstimate = +(supplied * (pool.account.supply_apy_bps() / 10_000) / 12).toFixed(4)
            const lendMeta = getTokenMeta(new PublicKey(pool.account.lend_mint).toBase58())
            const collateralMeta = getTokenMeta(new PublicKey(pool.account.collateral_mint).toBase58())

            return [{
                id: pool.publicKey.toBase58(),
                asset: lendMeta?.symbol ?? 'Unknown',
                icon: lendMeta?.icon ?? '',
                collateralAsset: collateralMeta?.symbol ?? 'Unknown',
                collateralIcon: collateralMeta?.icon ?? '',
                supplied,
                apy: pool.account.supply_apy_bps() / 100,
                earned: earnedEstimate,
                health,
                collateralEnabled: true,
            } satisfies LendPosition]
        })
    }, [enabled, pools, balances, decimalsMap])

    return { data, isLoading: enabled && (poolsLoading || balancesLoading) }
}

// ─── Borrow Positions ──────────────────────────────────────────────────────────
// Derived from on-chain UserPosition accounts where debtShares > 0.

export function useBorrowPositions(enabled = true) {
    const authority = useWalletPublicKey()
    const { data: pools = [], isLoading: poolsLoading } = useValidLendingAccounts()
    const { data: userPositions = [], isLoading: positionsLoading } =
        useUserPositionsByAuthority(enabled ? authority : null)

    const allMints = useMemo(() => [
        ...pools.map((p) => new PublicKey(p.account.lend_mint)),
        ...pools.map((p) => new PublicKey(p.account.collateral_mint)),
    ], [pools])
    const decimalsMap = useMintDecimalsMap(allMints)

    const data = useMemo<BorrowPosition[]>(() => {
        if (!enabled || !userPositions.length || !pools.length) return []

        return userPositions.flatMap((pos) => {
            if (!pos.has_debt() && !pos.has_collateral()) return []

            const pool = pools.find((p) => p.publicKey.equals(new PublicKey(pos.pool)))
            if (!pool) return []

            const lendDecimals = decimalsMap.get(new PublicKey(pool.account.lend_mint).toBase58()) ?? 6
            const collateralDecimals = decimalsMap.get(new PublicKey(pool.account.collateral_mint).toBase58()) ?? 6
            const debtRaw = pool.account.debt_amount(pos) ?? 0n
            const debtAmount = token_amount_to_f64(debtRaw, lendDecimals)
            const collateralAmount = token_amount_to_f64(pos.collateral_deposited, collateralDecimals)

            const ltvBps = pool.account.ltv(pos)
            const healthFactorBps = pool.account.health_factor(pos)
            const liqPriceBps = pool.account.liq_price(pos)

            const lendMeta = getTokenMeta(new PublicKey(pool.account.lend_mint).toBase58())
            const collateralMeta = getTokenMeta(new PublicKey(pool.account.collateral_mint).toBase58())

            return [{
                id: pool.publicKey.toBase58(),
                poolId: pool.publicKey.toBase58(),
                collateralAsset: collateralMeta?.symbol ?? 'Unknown',
                collateralIcon: collateralMeta?.icon ?? '',
                collateralAmount,
                borrowedAsset: lendMeta?.symbol ?? 'Unknown',
                borrowedIcon: lendMeta?.icon ?? '',
                debtAmount,
                borrowAPY: pool.account.borrow_apy_bps() / 100,
                supplyAPY: pool.account.supply_apy_bps() / 100,
                ltv: ltvBps != null ? ltvBps / 100 : null,
                liqPrice: liqPriceBps != null ? liqPriceBps / 10000 : null,
                healthFactor: healthFactorBps != null ? healthFactorBps / 10000 : null,
            } satisfies BorrowPosition]
        })
    }, [enabled, userPositions, pools, decimalsMap])

    return { data, isLoading: enabled && (poolsLoading || positionsLoading) }
}

// ─── Multiply Positions ────────────────────────────────────────────────────────
// Leveraged positions share the same on-chain account type (UserPosition) as
// borrow positions — there is no on-chain marker to distinguish them.
// This hook re-exports borrow positions as multiply-compatible entries using
// available on-chain metrics (collateral as position size, debt ratio as multiplier).

export function useMultiplyPositions(enabled = true) {
    const { data: borrowPositions, isLoading } = useBorrowPositions(enabled)
    const { data: pools = [] } = useValidLendingAccounts()

    const data = useMemo<MultiplyPosition[]>(() => {
        return borrowPositions.map((pos) => {
            const pool = pools.find((p) => p.publicKey.toBase58() === pos.poolId)
            // Effective multiplier: how many times the net equity is leveraged.
            // net equity = collateral − debt; multiplier = collateral / equity.
            const netEquity = Math.max(pos.collateralAmount - pos.debtAmount, 0.01)
            const multiplier = Math.min(pos.collateralAmount / netEquity, 30)
            const leverageBps = Math.round(multiplier * 10_000)
            const netAPY = pool
                ? pool.account.leveraged_net_apy_bps(leverageBps) / 100
                : 0

            return {
                id: pos.id,
                poolId: pos.poolId,
                // The collateral asset is what the user deposited (the "leveraged" side).
                // The borrowed asset is the debt token they owe.
                asset: pos.collateralAsset,
                icon: pos.collateralIcon,
                debtAsset: pos.borrowedAsset,
                debtIcon: pos.borrowedIcon,
                multiplier: +multiplier.toFixed(2),
                netAPY: +netAPY.toFixed(2),
                // positionSize = full collateral value (not derived via LTV round-trip)
                positionSize: +pos.collateralAmount.toFixed(2),
                entryPrice: 0,
                currentPrice: 0,
                liqPrice: pos.liqPrice,
                pnl: 0,
                pnlPct: 0,
            } satisfies MultiplyPosition
        })
    }, [borrowPositions, pools])

    return { data, isLoading }
}

// ─── Portfolio Summary ─────────────────────────────────────────────────────────
// Net value and totals are derived from real chain data.
// The 90-day history chart remains illustrative (no on-chain history).

export function usePortfolioSummary(enabled = true) {
    const { data: lendPositions, isLoading: lendLoading } = useLendPositions(enabled)
    const { data: borrowPositions, isLoading: borrowLoading } = useBorrowPositions(enabled)

    const data = useMemo<PortfolioSummary | undefined>(() => {
        if (!enabled) return undefined

        const totalSupplied = lendPositions.reduce((s, p) => s + p.supplied, 0)
        const totalDebt = borrowPositions.reduce((s, p) => s + p.debtAmount, 0)
        const netValue = Math.max(0, totalSupplied - totalDebt)

        // Generate raw mock history with a stable reference value (PORTFOLIO_START).
        // Then scale every point proportionally so the last point equals the real
        // net value. This preserves the visual shape while avoiding the "cliff"
        // caused by overriding only the last point when history and current value
        // are on different scales.
        const rawHistory = generatePortfolioHistory(90)
        const rawLast = rawHistory[rawHistory.length - 1].value
        const scale = netValue > 0 && rawLast > 0 ? netValue / rawLast : 1
        const history: PortfolioHistoryPoint[] = rawHistory.map((p) => ({
            ...p,
            value: Math.round(p.value * scale),
        }))

        const last = history[history.length - 1]
        const ago30 = history[history.length - 31]
        const change30d = last.value - ago30.value
        const change30dPct = ago30.value > 0 ? (change30d / ago30.value) * 100 : 0

        // Leveraged exposure = sum of collateral values across all borrow positions.
        // Using collateralAmount directly avoids the ltv round-trip (debt/ltv)
        // which can amplify floating-point errors at low ltv values.
        const leveragedExposure = borrowPositions.reduce((s, p) => s + p.collateralAmount, 0)

        return {
            netValue,
            totalSupplied,
            totalDebt,
            leveragedExposure,
            change30d,
            change30dPct,
            history,
        }
    }, [enabled, lendPositions, borrowPositions])

    return { data, isLoading: lendLoading || borrowLoading }
}


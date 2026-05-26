import { getPoolMeta } from '@/config/poolRegistry'
import { PoolAccount } from '@jbl/wasm-lib'
import { PublicKey } from '@solana/web3.js'
import type { Pool } from '@/types/pool'

const DECIMALS_FACTOR = 10 ** 6

/**
 * Map on-chain PoolAccount to the display-friendly Pool shape used by UI
 * components. Amounts are converted from 6-decimal raw units to human-readable
 * values; APY and utilization are derived via wasm methods on PoolAccount.
 *
 * `address` and `id` are both the pool's own PublicKey base-58 string so
 * routing with `/pool/:address` resolves back to the same account.
 */
export function poolDataToDisplayPool(publicKey: PublicKey, pd: PoolAccount): Pool {
    const addr = publicKey.toBase58()
    const meta = getPoolMeta(addr)

    return {
        id: addr,
        address: addr,
        ...meta,
        category: meta.category,
        binancePerp: meta.binancePerp,
        account: pd,
        supplyAPY: pd.supply_apy_bps() / 100,
        borrowAPY: pd.borrow_apy_bps() / 100,
        totalSupplied: Number(pd.total_supply_assets) / DECIMALS_FACTOR,
        totalBorrowed: Number(pd.total_borrow_assets) / DECIMALS_FACTOR,
        totalCollateral: Number(pd.total_collateral_deposited) / DECIMALS_FACTOR,
        utilization: pd.utilization_bps() / 100,
        availableLiquidity: Number(pd.available_liquidity()) / DECIMALS_FACTOR,
    }
}

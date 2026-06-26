import { getPoolMeta } from '@/config/poolRegistry'
import { PoolWithIrm } from '@jbl/wasm-lib'
import { PublicKey } from '@solana/web3.js'
import type { Pool } from '@/types/pool'

/**
 * Map on-chain PoolAccount to the display-friendly Pool shape used by UI
 * components. Raw lend-token amounts are divided by `10 ** lendDecimals`;
 * APY and utilization are derived via wasm methods on PoolAccount.
 *
 * `address` and `id` are both the pool's own PublicKey base-58 string so
 * routing with `/pool/:address` resolves back to the same account.
 */
export function poolDataToDisplayPool(publicKey: PublicKey, pd: PoolWithIrm, lendDecimals: number): Pool {
    const addr = publicKey.toBase58()
    const meta = getPoolMeta(addr)
    const lendScale = 10 ** lendDecimals

    return {
        id: addr,
        address: addr,
        ...meta,
        category: meta.category,
        binancePerp: meta.binancePerp,
        account: pd,
        supplyAPY: pd.supply_apy_bps() / 100,
        borrowAPY: pd.borrow_apy_bps() / 100,
        totalSupplied: Number(pd.total_supply_assets) / lendScale,
        totalBorrowed: Number(pd.total_borrow_assets) / lendScale,
        utilization: pd.utilization_bps() / 100,
        availableLiquidity: Number(pd.available_liquidity()) / lendScale,
    }
}

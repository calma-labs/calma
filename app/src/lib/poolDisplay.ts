import { getTokenMeta } from '@/lib/tokenRegistry'
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
    const lendMintAddr = new PublicKey(pd.lend_mint).toBase58()
    const collateralMintAddr = new PublicKey(pd.collateral_mint).toBase58()

    const meta = getTokenMeta(lendMintAddr)
    const collateralMeta = getTokenMeta(collateralMintAddr)

    const lendScale = 10 ** lendDecimals

    return {
        id: addr,
        address: addr,
        name: meta?.name ?? 'Unknown',
        symbol: meta?.symbol ?? 'Unknown',
        icon: meta?.icon ?? '',
        category: meta?.category ?? 'volatile',
        binancePerp: meta?.binancePerp,
        lendSymbol: meta?.symbol ?? 'Unknown',
        lendIcon: meta?.icon ?? '',
        collateralSymbol: collateralMeta?.symbol ?? 'Unknown',
        collateralIcon: collateralMeta?.icon ?? '',
        account: pd,
        supplyAPY: pd.supply_apy_bps() / 100,
        borrowAPY: pd.borrow_apy_bps() / 100,
        totalSupplied: Number(pd.total_supply_assets) / lendScale,
        totalBorrowed: Number(pd.total_borrow_assets) / lendScale,
        utilization: pd.utilization_bps() / 100,
        availableLiquidity: Number(pd.available_liquidity()) / lendScale,
    }
}

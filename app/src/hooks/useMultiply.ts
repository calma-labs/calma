import { useValidLendingAccounts } from "@/hooks/program/useValidLendingAccounts";
import { useMintDecimalsMap } from "@/hooks/useMintDecimals";
import { poolDataToDisplayPool } from "@/lib/poolDisplay";
import type { MultiplyMeta, Pool } from "@/types/pool";
import { PublicKey } from "@solana/web3.js";
import { useMemo } from "react";

/** Absolute ceiling / fallback when a pool's LTV doesn't yield a finite cap. */
export const MAX_MULTIPLY = 30;

/** On-chain price scale — `oracle_price` is lend-raw per collateral-raw × this. */
const PRICE_SCALE = 1_000_000;

/**
 * Real max leverage a single flash-loop can reach for a pool.
 *
 * The borrow gate caps debt at `collateral × (oracle_price/PRICE_SCALE) × ltv`
 * (`crates/math/src/core.rs` `max_borrow_capacity`), and the loop ends at token
 * ratio `(L−1)/L` collateral→debt. With `p = oracle_price/PRICE_SCALE` (value of
 * one collateral token in lend units, decimals included), the gate binds when
 * `(L−1)/L ≤ p·LTV`, so `L ≤ 1/(1 − p·LTV)`. Ignoring `p` (the old `1/(1−LTV)`)
 * over-states the cap whenever collateral is priced below the lend token.
 *
 * We shave a 1% margin (9 bps flash fee + float rounding at the boundary) and
 * floor to a 0.1 step so the top of the slider never reverts on-chain. `oraclePrice`
 * of 0 (feed not loaded) falls back to a 1:1 price so the cap stays sensible.
 */
export function maxLeverageForLtv(ltvPercent: number, oraclePrice: number): number {
  if (ltvPercent <= 0) return 1;
  const priceFactor =
    Number.isFinite(oraclePrice) && oraclePrice > 0 ? oraclePrice / PRICE_SCALE : 1;
  const effLtv = (ltvPercent / 100) * priceFactor;
  if (effLtv >= 1) return MAX_MULTIPLY; // gate never binds → cap at fallback ceiling
  return Math.max(1, Math.floor((1 / (1 - effLtv)) * 0.99 * 10) / 10);
}

export interface MultiplyStrategy extends Pool {
  meta: MultiplyMeta;
}

/**
 * Derives a MultiplyMeta from an already-mapped Pool.
 * Max multiplier is the pool's real cap (`1/(1−LTV)`, via `maxLeverageForLtv`).
 * Max net APY is computed at that leverage: L×supplyAPY − (L−1)×borrowAPY.
 */
export function buildMultiplyMeta(pool: Pool): MultiplyMeta {
  const maxMultiplier = maxLeverageForLtv(pool.account.ltv_percent, Number(pool.account.oracle_price));
  const maxNetAPY = pool.account.leveraged_net_apy(maxMultiplier);
  return {
    maxMultiplier,
    maxNetAPY,
    // Lend token is the debt in a multiply position (borrowed against collateral)
    debtSymbol: pool.lendSymbol,
    debtIcon: pool.lendIcon,
  };
}

/** Returns all on-chain pools as multiply strategies (one strategy per pool). */
export function useMultiplyStrategies() {
  const { data: poolsData = [], isLoading } = useValidLendingAccounts();

  const lendMints = useMemo(
    () => poolsData.map((pd) => new PublicKey(pd.account.lend_mint)),
    [poolsData],
  );
  const decimalsMap = useMintDecimalsMap(lendMints);

  const strategies = useMemo<MultiplyStrategy[]>(
    () =>
      poolsData.map((pd) => {
        const lendDecimals = decimalsMap.get(new PublicKey(pd.account.lend_mint).toBase58()) ?? 6;
        const pool = poolDataToDisplayPool(pd.publicKey, pd.account, lendDecimals);
        return { ...pool, meta: buildMultiplyMeta(pool) };
      }),
    [poolsData, decimalsMap],
  );

  return { data: strategies, isLoading };
}

/** Returns a single multiply strategy by pool address. */
export function useMultiplyStrategy(address: string | undefined) {
  const { data: strategies, isLoading } = useMultiplyStrategies();

  const strategy = useMemo<MultiplyStrategy | null>(
    () =>
      address
        ? (strategies.find((s) => s.address === address) ?? null)
        : null,
    [strategies, address],
  );

  return { data: strategy, isLoading };
}

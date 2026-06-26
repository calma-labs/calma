import { useValidLendingAccounts } from "@/hooks/program/useValidLendingAccounts";
import { useMintDecimalsMap } from "@/hooks/useMintDecimals";
import { poolDataToDisplayPool } from "@/lib/poolDisplay";
import type { MultiplyMeta, Pool } from "@/types/pool";
import { PublicKey } from "@solana/web3.js";
import { useMemo } from "react";

export const MAX_MULTIPLY = 30;

export interface MultiplyStrategy extends Pool {
  meta: MultiplyMeta;
}

/**
 * Derives a MultiplyMeta from an already-mapped Pool.
 * Max multiplier is hardcoded at 30×.
 * Max net APY is computed at full leverage: L×supplyAPY − (L−1)×borrowAPY.
 */
export function buildMultiplyMeta(pool: Pool): MultiplyMeta {
  const maxNetAPY = pool.account.leveraged_net_apy(MAX_MULTIPLY);
  return {
    maxMultiplier: MAX_MULTIPLY,
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

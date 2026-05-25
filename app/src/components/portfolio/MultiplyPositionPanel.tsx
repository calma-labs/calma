import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import type { PoolAccount } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { useMemo, useState } from "react";
import { PnlCell } from "../common/PnlCell";
import { PositionActionButton } from "../common/PositionActionButton";
import { ClosePositionModal } from "./ClosePositionModal";
import {
  ManagePositionModal,
  type ManageMultiplyPosition,
} from "./ManagePositionModal";

// ─── Leverage math ─────────────────────────────────────────────────────────────

/**
 * Computes effective leverage from on-chain collateral and debt.
 * Formula: leverage = collateral / (collateral − debt × price), price = 1.
 * Returns 1 when there is no debt.
 */
function computeLeverage(collateral: number, debt: number): number {
  const equity = collateral - debt;
  if (equity <= 0 || collateral <= 0) return 1;
  return collateral / equity;
}

// ─── Main component ───────────────────────────────────────────────────────────

type ModalState =
  | { type: "manage"; pos: ManageMultiplyPosition }
  | { type: "close" }
  | null;

interface MultiplyPositionPanelProps {
  pool: Pool;
  poolData: PoolAccount;
  connected: boolean;
}

/**
 * Shows the user's active multiply (leveraged) position for the given pool.
 * A multiply position is identified by having both collateral deposited AND
 * outstanding debt shares at the same time.
 */
export function MultiplyPositionPanel({
  pool,
  poolData,
  connected,
}: MultiplyPositionPanelProps) {
  const [modal, setModal] = useState<ModalState>(null);
  const { wallet } = useWalletConnection();

  const poolPubKey = useMemo(() => {
    try {
      return new PublicKey(pool.address);
    } catch {
      return null;
    }
  }, [pool.address]);

  const walletPubKey = useMemo(() => {
    if (!wallet) return null;
    try {
      return new PublicKey(wallet.account.publicKey);
    } catch {
      return null;
    }
  }, [wallet]);

  const { data: userPosition, isLoading } = useUserPosition(
    poolPubKey,
    walletPubKey,
  );
  const { data: collateralDecimals } = useMintDecimals(new PublicKey(poolData.collateral_mint));
  const { data: lendDecimals } = useMintDecimals(new PublicKey(poolData.lend_mint));

  // Derive human-readable amounts and position metrics
  const position = useMemo(() => {
    if (!userPosition) return null;

    // A multiply position has both collateral and debt
    if (!userPosition.has_collateral() || !userPosition.has_debt()) return null;

    const colDec = collateralDecimals ?? 6;
    const lndDec = lendDecimals ?? 6;

    const collateralUi =
      Number(userPosition.collateral_deposited) / 10 ** colDec;

    const debtRaw = userPosition.debt_amount(
      poolData.total_borrowed,
      poolData.total_debt_shares,
    );
    const debtUi = Number(debtRaw) / 10 ** lndDec;

    const leverage = computeLeverage(collateralUi, debtUi);
    const netAPY = Math.max(
      0,
      leverage * pool.supplyAPY - (leverage - 1) * pool.borrowAPY,
    );

    return {
      collateralUi,
      debtUi,
      debtRaw,
      leverage,
      netAPY,
      collateralFormatted: userPosition.format_collateral(colDec),
      debtFormatted: userPosition.format_debt(
        poolData.total_borrowed,
        poolData.total_debt_shares,
        lndDec,
      ),
    };
  }, [userPosition, collateralDecimals, lendDecimals, poolData, pool]);

  if (!connected) return null;
  if (isLoading) {
    return (
      <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.02] px-4 py-6 flex items-center gap-2">
        <div className="h-1.5 w-1.5 rounded-full bg-[#c698e5] animate-pulse" />
        <span className="text-[11px] text-[#efe0f7]/30">Loading position…</span>
      </div>
    );
  }
  if (!position) return null;

  // Build ManageMultiplyPosition from real data (price = 1 assumed)
  const managePos: ManageMultiplyPosition = {
    asset: pool.lendSymbol,
    icon: pool.lendIcon,
    debtAsset: pool.collateralSymbol,
    multiplier: position.leverage,
    netAPY: position.netAPY,
    positionSize: position.collateralUi,
    entryPrice: 1,
    currentPrice: 1,
    liqPrice: 1 / position.leverage,
    pnl: 0,
    pnlPct: 0,
  };

  return (
    <>
      <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.02] overflow-hidden">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-[#c698e5]/10">
          <div className="h-1.5 w-1.5 rounded-full bg-[#c698e5]" />
          <span className="text-[11px] font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            My Positions
          </span>
        </div>

        {/* Position row */}
        <div className="flex flex-wrap items-center gap-x-10 gap-y-2 px-4 py-3.5">
          {/* Asset */}
          <div className="flex items-center gap-2 min-w-[90px]">
            <img
              src={pool.lendIcon}
              alt={pool.lendSymbol}
              className="h-6 w-6 rounded-full"
            />
            <div>
              <p className="text-sm font-semibold text-[#efe0f7]">
                {pool.lendSymbol}
              </p>
              <p className="text-[10px] text-[#efe0f7]/35">Multiply</p>
            </div>
          </div>

          {/* Multiplier */}
          <div className="flex flex-col min-w-[55px]">
            <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">
              Multiplier
            </span>
            <span className="text-sm font-bold tabular-nums text-[#c698e5]">
              {position.leverage.toFixed(2)}×
            </span>
          </div>

          {/* Net APY */}
          <div className="flex flex-col min-w-[60px]">
            <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">
              Net APY
            </span>
            <span className="text-sm font-semibold tabular-nums text-[#34d399]">
              {position.netAPY.toFixed(2)}%
            </span>
          </div>

          {/* Collateral */}
          <div className="flex flex-col min-w-[100px]">
            <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">
              Collateral
            </span>
            <span className="text-sm font-semibold tabular-nums text-[#efe0f7]">
              {position.collateralFormatted}{" "}
              <span className="text-xs text-[#efe0f7]/40">
                {pool.collateralSymbol}
              </span>
            </span>
          </div>

          {/* Debt */}
          <div className="flex flex-col min-w-[100px]">
            <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Debt</span>
            <span className="text-sm font-semibold tabular-nums text-[#efe0f7]">
              {position.debtFormatted}{" "}
              <span className="text-xs text-[#efe0f7]/40">
                {pool.lendSymbol}
              </span>
            </span>
          </div>

          {/* P&L — not computable without price history; shown as placeholder */}
          <div className="flex flex-col min-w-[90px]">
            <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">P&L</span>
            <PnlCell pnl={0} pct={0} />
          </div>

          {/* Actions */}
          <div className="flex items-center gap-2 ml-auto">
            {/* <PositionActionButton
              label="Manage"
              onClick={() => setModal({ type: "manage", pos: managePos })}
            /> */}
            <PositionActionButton
              label="Close"
              onClick={() => setModal({ type: "close" })}
            />
          </div>
        </div>
      </div>

      {modal?.type === "manage" && (
        <ManagePositionModal
          position={modal.pos}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.type === "close" && userPosition && (
        <ClosePositionModal
          pool={pool}
          poolData={poolData}
          userPosition={userPosition}
          position={managePos}
          onClose={() => setModal(null)}
        />
      )}
    </>
  );
}

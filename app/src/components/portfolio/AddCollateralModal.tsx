import { cn } from "@/lib/utils";
import type { PoolWithIrm, UserPositionAccount } from "@jbl/wasm-lib";
import { AlertTriangle, Info, Wallet, X } from "lucide-react";
import { useMemo, useState } from "react";

export interface AddCollateralPosition {
  collateralAsset: string;
  collateralIcon: string;
  borrowedAsset: string;
  /** Current LTV in percent (e.g. 45.2) — displayed as the "before" value. */
  ltv: number;
  liqPrice: number;
  /** Current health factor (e.g. 2.1) — displayed as the "before" value. */
  healthFactor: number;
  /** Wallet balance of the collateral token in display units. */
  walletBalance: number;
}

interface AddCollateralModalProps {
  position: AddCollateralPosition;
  poolData: PoolWithIrm;
  userPosition: UserPositionAccount;
  /** Decimal count of the collateral token (used to convert display → raw). */
  collateralDecimals: number;
  onClose: () => void;
  onAddCollateral?: (amount: number) => Promise<void>;
}

export function AddCollateralModal({
  position,
  poolData,
  userPosition,
  collateralDecimals,
  onClose,
  onAddCollateral,
}: AddCollateralModalProps) {
  const [amount, setAmount] = useState("");

  const numAmount = parseFloat(amount) || 0;
  const walletBalance = position.walletBalance;

  const { projectedLTV, projectedHF } = useMemo(() => {
    if (numAmount <= 0) return { projectedLTV: null, projectedHF: null };
    const addedRaw = BigInt(Math.round(numAmount * 10 ** collateralDecimals));
    const ltvBps = poolData.projected_ltv_after_deposit(userPosition, addedRaw);
    const hfBps = poolData.projected_health_factor_after_deposit(userPosition, addedRaw);
    return {
      projectedLTV: ltvBps != null ? ltvBps / 100 : null,
      projectedHF: hfBps != null ? hfBps / 10_000 : null,
    };
  }, [numAmount, collateralDecimals, poolData, userPosition]);

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!numAmount || numAmount <= 0) return;
    if (onAddCollateral) await onAddCollateral(numAmount);
    onClose();
  }

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-sm rounded-2xl border border-[#c698e5]/15 bg-[#1a0d24] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-[#c698e5]/10">
          <div className="flex items-center gap-3">
            <img
              src={position.collateralIcon}
              alt={position.collateralAsset}
              className="h-9 w-9 rounded-full ring-1 ring-[#c698e5]/20"
            />
            <div>
              <p className="text-sm font-semibold text-[#efe0f7]">
                Add Collateral
              </p>
              <p className="text-[11px] text-[#efe0f7]/35">
                {position.collateralAsset} · borrowed {position.borrowedAsset}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-[#efe0f7]/30 hover:bg-[#c698e5]/10 hover:text-[#efe0f7] transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 pt-4 flex flex-col gap-4">
          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between px-1">
              <span className="text-[11px] text-[#efe0f7]/40">
                Collateral amount
              </span>
              <button
                onClick={() => setAmount(String(walletBalance))}
                className="flex items-center gap-1 text-[11px] text-[#efe0f7]/35 hover:text-[#c698e5] transition-colors cursor-pointer"
              >
                <Wallet className="h-3 w-3" />
                <span className="tabular-nums text-[#efe0f7]/55">
                  {walletBalance.toLocaleString("en-US", {
                    minimumFractionDigits: 4,
                    maximumFractionDigits: 4,
                  })}{" "}
                  {position.collateralAsset}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/5 px-3.5 py-3 transition-colors focus-within:border-[#c698e5]/40">
              <img
                src={position.collateralIcon}
                alt={position.collateralAsset}
                className="h-5 w-5 rounded-full flex-shrink-0"
              />
              <input
                type="number"
                min="0"
                max={walletBalance}
                placeholder="0.00"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-[#efe0f7] placeholder-[#efe0f7]/20 outline-none tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-[11px] font-semibold text-[#efe0f7]/40 flex-shrink-0">
                {position.collateralAsset}
              </span>
            </div>
          </div>

          {/* Percentage shortcuts */}
          <div className="flex gap-1.5">
            {[25, 50, 75, 100].map((p) => (
              <button
                key={p}
                onClick={() =>
                  setAmount(((walletBalance * p) / 100).toFixed(4))
                }
                className="flex-1 rounded-lg border border-[#c698e5]/15 py-1.5 text-[11px] font-medium text-[#efe0f7]/35 hover:border-[#c698e5]/35 hover:text-[#c698e5] transition-all cursor-pointer"
              >
                {p}%
              </button>
            ))}
          </div>

          {/* Stats */}
          <div className="rounded-xl border border-[#c698e5]/10 divide-y divide-[#c698e5]/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                LTV ratio
              </span>
              <div className="flex items-center gap-1.5">
                <span className="text-xs tabular-nums text-[#efe0f7]/40">
                  {position.ltv.toFixed(1)}%
                </span>
                {projectedLTV != null && (
                  <>
                    <span className="text-[#efe0f7]/20">→</span>
                    <span className="text-xs font-semibold tabular-nums text-[#34d399]">
                      {projectedLTV.toFixed(1)}%
                    </span>
                  </>
                )}
              </div>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                Liq. price
              </span>
              <span className="text-xs font-semibold tabular-nums text-[#efe0f7]/70">
                ${position.liqPrice.toFixed(2)}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <AlertTriangle className="h-3 w-3" />
                Health factor
              </span>
              <div className="flex items-center gap-1.5">
                <span
                  className={cn(
                    "text-xs font-semibold tabular-nums",
                    position.healthFactor >= 2.5
                      ? "text-[#34d399]"
                      : position.healthFactor >= 1.5
                      ? "text-[#f0a854]"
                      : "text-[#d45677]",
                  )}
                >
                  {position.healthFactor.toFixed(2)}
                </span>
                {projectedHF != null && (
                  <>
                    <span className="text-[#efe0f7]/20">→</span>
                    <span
                      className={cn(
                        "text-xs font-semibold tabular-nums",
                        projectedHF >= 2.5
                          ? "text-[#34d399]"
                          : projectedHF >= 1.5
                          ? "text-[#f0a854]"
                          : "text-[#d45677]",
                      )}
                    >
                      {projectedHF.toFixed(2)}
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          <button
            disabled={!numAmount || numAmount <= 0 || numAmount > walletBalance}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer",
              numAmount > 0 && numAmount <= walletBalance
                ? "bg-[#c698e5] text-[#17081f] hover:bg-[#d4aeee]"
                : "bg-[#c698e5]/12 text-[#efe0f7]/20 cursor-not-allowed",
            )}
          >
            Add {position.collateralAsset} Collateral
          </button>
        </div>
      </div>
    </div>
  );
}

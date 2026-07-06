import { cn } from "@/lib/utils";
import { Info, Loader2, X } from "lucide-react";
import { useState } from "react";

export interface RepayPosition {
  collateralAsset: string;
  collateralIcon: string;
  borrowedAsset: string;
  borrowedIcon: string;
  debtAmount: number;
  borrowAPY: number;
  /** User's lend-token wallet balance — caps how much they can actually repay. */
  walletBalance?: number;
  /** Raw exact debt amount as string to avoid floating point precision issues when repaying max */
  rawDebtAmount?: string;
}

interface RepayModalProps {
  position: RepayPosition;
  onClose: () => void;
  onRepay?: (amount: number, rawAmount?: string) => Promise<void>;
  isPending?: boolean;
}

export function RepayModal({
  position,
  onClose,
  onRepay,
  isPending,
}: RepayModalProps) {
  const [amount, setAmount] = useState("");

  const numAmount = parseFloat(amount) || 0;
  const maxRepay = Math.min(
    position.debtAmount,
    position.walletBalance ?? position.debtAmount,
  );
  const remaining = Math.max(maxRepay - numAmount, 0);

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!numAmount || numAmount <= 0) return;
    // Pass raw amount if repaying max (to avoid floating point precision issues)
    const isMaxRepay = numAmount >= maxRepay;
    if (onRepay) await onRepay(numAmount, isMaxRepay ? position.rawDebtAmount : undefined);
    onClose();
  }

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-sm rounded-2xl border border-surface-accent/15 bg-modal-bg overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-surface-accent/10">
          <div className="flex items-center gap-3">
            <div className="relative">
              <img
                src={position.borrowedIcon}
                alt={position.borrowedAsset}
                className="h-9 w-9 rounded-full ring-1 ring-surface-accent/20"
              />
              {/* style-exception: 18px icon size required for stacked icon overlap design */}
              <img
                src={position.collateralIcon}
                alt={position.collateralAsset}
                className="absolute -bottom-0.5 -right-0.5 h-[18px] w-[18px] rounded-full ring-1 ring-modal-bg"
              />
            </div>
            <div>
              <p className="text-sm font-semibold text-surface-foreground">
                Repay {position.borrowedAsset}
              </p>
              <p className="text-xs text-surface-foreground/35">
                Collateral: {position.collateralAsset}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-surface-foreground/30 hover:bg-surface-accent/10 hover:text-surface-foreground transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 pt-4 flex flex-col gap-4">
          {/* Debt summary */}
          <div className="rounded-xl border border-destructive/20 bg-destructive/5 px-3.5 py-2.5 flex items-center justify-between">
            <span className="text-xs text-surface-foreground/50">Outstanding debt</span>
            <span className="text-sm font-bold tabular-nums text-destructive">
              ${position.debtAmount.toLocaleString()} {position.borrowedAsset}
            </span>
          </div>

          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between px-1">
              <span className="text-xs text-surface-foreground/40">
                Repay amount
              </span>
              <button
                onClick={() => setAmount(String(maxRepay))}
                className="text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer"
              >
                Max:{" "}
                <span className="tabular-nums text-surface-foreground/55">
                  {maxRepay.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 2,
                  })}{" "}
                  {position.borrowedAsset}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-surface-accent/15 bg-surface-accent/5 px-3.5 py-3 transition-colors focus-within:border-surface-accent/40">
              <img
                src={position.borrowedIcon}
                alt={position.borrowedAsset}
                className="h-5 w-5 rounded-full flex-shrink-0"
              />
              <input
                type="number"
                min="0"
                max={maxRepay}
                placeholder="0.00"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-surface-foreground placeholder-surface-foreground/20 tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-xs font-semibold text-surface-foreground/40 flex-shrink-0">
                {position.borrowedAsset}
              </span>
            </div>
          </div>

          {/* Percentage shortcuts */}
          <div className="flex gap-1.5">
            {[25, 50, 75, 100].map((p) => (
              <button
                key={p}
                onClick={() => setAmount(String((maxRepay * p) / 100))}
                className="flex-1 rounded-lg border border-surface-accent/15 py-1.5 text-xs font-medium text-surface-foreground/35 hover:border-surface-accent/35 hover:text-surface-accent transition-all cursor-pointer"
              >
                {p}%
              </button>
            ))}
          </div>

          {/* Stats */}
          <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Borrow APY
              </span>
              <span className="text-xs font-semibold text-destructive">
                {position.borrowAPY.toFixed(2)}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Remaining debt
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/70">
                {remaining.toLocaleString("en-US", {
                  maximumFractionDigits: 6,
                })}{" "}
                {position.borrowedAsset}
              </span>
            </div>
          </div>

          <button
            disabled={
              !numAmount || numAmount <= 0 || numAmount > maxRepay || isPending
            }
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer flex items-center justify-center gap-2",
              numAmount > 0 && numAmount <= maxRepay && !isPending
                ? "bg-destructive text-white hover:bg-destructive/80"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            Repay {position.borrowedAsset}
          </button>
        </div>
      </div>
    </div>
  );
}

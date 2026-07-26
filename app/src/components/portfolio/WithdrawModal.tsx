import { cn } from "@/lib/utils";
import { AlertTriangle, Info, Loader2, X } from "lucide-react";
import { useState } from "react";

export interface WithdrawPosition {
  asset: string;
  icon: string;
  supplied: number;
  apy: number;
  collateralEnabled: boolean;
  /** Raw exact amount as string to avoid floating point precision issues when withdrawing max */
  rawSupplied?: string;
}

interface WithdrawModalProps {
  position: WithdrawPosition;
  onClose: () => void;
  disabled?: boolean;
  onWithdraw?: (amount: string, rawAmount?: string) => Promise<void>;
  isPending?: boolean;
}

export function WithdrawModal({
  position,
  onClose,
  onWithdraw,
  isPending,
}: WithdrawModalProps) {
  const [amount, setAmount] = useState("");

  const numAmount = parseFloat(amount) || 0;
  const maxWithdraw = position.supplied;
  const remaining = Math.max(maxWithdraw - numAmount, 0);

  // Format amount with pretty dot notation (2 decimal places minimum)
  function formatAmount(value: number): string {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 6,
      useGrouping: false,
    });
  }

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!numAmount || numAmount <= 0) return;
    // Pass raw amount if withdrawing max (to avoid floating point precision issues)
    const isMaxWithdrawal = numAmount >= maxWithdraw;
    if (onWithdraw) await onWithdraw(amount, isMaxWithdrawal ? position.rawSupplied : undefined);
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
            <img
              src={position.icon}
              alt={position.asset}
              className="h-9 w-9 rounded-full ring-1 ring-surface-accent/20"
            />
            <div>
              <p className="text-sm font-semibold text-surface-foreground">
                Withdraw {position.asset}
              </p>
              <p className="text-xs text-surface-foreground/35">
                Supplied:{" "}
                <span className="tabular-nums text-surface-foreground/60">
                  {position.supplied.toLocaleString("en-US", {
                    maximumFractionDigits: 9,
                  })}{" "}
                  {position.asset}
                </span>
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
          {/* Collateral warning */}
          {position.collateralEnabled && (
            <div className="rounded-xl border border-warning/20 bg-warning/5 px-3.5 py-2.5 flex items-start gap-2">
              <AlertTriangle className="h-3.5 w-3.5 text-warning mt-0.5 flex-shrink-0" />
              <p className="text-xs text-warning/80 leading-relaxed">
                This asset is used as collateral. Withdrawing may affect your
                health factor.
              </p>
            </div>
          )}

          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between px-1">
              <span className="text-xs text-surface-foreground/40">
                Withdraw amount
              </span>
              <button
                onClick={() => setAmount(formatAmount(maxWithdraw))}
                className="text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer"
              >
                Max:{" "}
                <span className="tabular-nums text-surface-foreground/55">
                  {position.supplied.toLocaleString("en-US", {
                    maximumFractionDigits: 9,
                  })}{" "}
                  {position.asset}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-surface-accent/15 bg-surface-accent/5 px-3.5 py-3 transition-colors focus-within:border-surface-accent/40">
              <img
                src={position.icon}
                alt={position.asset}
                className="h-5 w-5 rounded-full flex-shrink-0"
              />
              <input
                type="number"
                min="0"
                max={maxWithdraw}
                placeholder="0.00"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-surface-foreground placeholder-surface-foreground/20 tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-xs font-semibold text-surface-foreground/40 flex-shrink-0">
                {position.asset}
              </span>
            </div>
          </div>

          {/* Percentage shortcuts */}
          <div className="flex gap-1.5">
            {[25, 50, 75, 100].map((p) => (
              <button
                key={p}
                onClick={() => setAmount(formatAmount((maxWithdraw * p) / 100))}
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
                Supply APY
              </span>
              <span className="text-xs font-semibold text-success">
                {position.apy.toFixed(2)}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Remaining supply
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/70">
                {remaining.toLocaleString("en-US", {
                  maximumFractionDigits: 9,
                })}{" "}
                {position.asset}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Collateral
              </span>
              <span
                className={cn(
                  "text-xs font-semibold",
                  position.collateralEnabled ? "text-success" : "text-surface-foreground/35",
                )}
              >
                {position.collateralEnabled ? "Enabled" : "Disabled"}
              </span>
            </div>
          </div>

          <button
            disabled={
              !numAmount ||
              numAmount <= 0 ||
              numAmount > maxWithdraw ||
              isPending
            }
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer flex items-center justify-center gap-2",
              numAmount > 0 && numAmount <= maxWithdraw && !isPending
                ? "bg-surface-accent text-surface hover:bg-surface-accent/80"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            Withdraw {position.asset}
          </button>
        </div>
      </div>
    </div>
  );
}

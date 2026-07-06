import { cn } from "@/lib/utils";
import { Info, Wallet, X } from "lucide-react";
import { useState } from "react";

const MOCK_WALLET_BALANCE = 5_000;

export interface SupplyMorePosition {
  asset: string;
  icon: string;
  supplied: number;
  apy: number;
  collateralEnabled: boolean;
}

interface SupplyMoreModalProps {
  position: SupplyMorePosition;
  onClose: () => void;
  /** future: pass onSupply(amount: number) => Promise<void> */
  onSupply?: (amount: number) => Promise<void>;
}

export function SupplyMoreModal({
  position,
  onClose,
  onSupply,
}: SupplyMoreModalProps) {
  const [amount, setAmount] = useState("");

  const numAmount = parseFloat(amount) || 0;
  const walletBalance = MOCK_WALLET_BALANCE;
  const totalAfter = position.supplied + numAmount;

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!numAmount || numAmount <= 0) return;
    if (onSupply) await onSupply(numAmount);
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
                Supply {position.asset}
              </p>
              <p className="text-xs text-surface-foreground/35">
                Currently supplied:{" "}
                <span className="tabular-nums text-surface-foreground/60">
                  ${position.supplied.toLocaleString()}
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
          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between px-1">
              <span className="text-xs text-surface-foreground/40">Amount</span>
              <button
                onClick={() => setAmount(String(walletBalance))}
                className="flex items-center gap-1 text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer"
              >
                <Wallet className="h-3 w-3" />
                <span className="tabular-nums text-surface-foreground/55">
                  {walletBalance.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 2,
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
                max={walletBalance}
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
                onClick={() =>
                  setAmount(((walletBalance * p) / 100).toFixed(2))
                }
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
                Total supplied after
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/70">
                $
                {totalAfter.toLocaleString("en-US", {
                  maximumFractionDigits: 2,
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
            disabled={!numAmount || numAmount <= 0 || numAmount > walletBalance}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer",
              numAmount > 0 && numAmount <= walletBalance
                ? "bg-surface-accent text-surface hover:bg-surface-accent/80"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            Supply {position.asset}
          </button>
        </div>
      </div>
    </div>
  );
}

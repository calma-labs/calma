import { cn } from "@/lib/utils";
import { AlertTriangle, Info, X, Zap } from "lucide-react";
import { useState } from "react";

// ─── Types ────────────────────────────────────────────────────────────────────

export interface ManageMultiplyPosition {
  asset: string;
  icon: string;
  debtAsset: string;
  multiplier: number;
  netAPY: number;
  positionSize: number;
  /** `null` when unknown — see the note on `MultiplyPosition.pnl`. No entry
   * price is recorded on-chain and there is no price index behind this modal. */
  entryPrice: number | null;
  currentPrice: number | null;
  liqPrice: number;
  pnl: number | null;
  pnlPct: number | null;
}

interface ManagePositionModalProps {
  position: ManageMultiplyPosition;
  onClose: () => void;
  /** future: pass onUpdate(multiplier: number) => Promise<void> */
  onUpdate?: (multiplier: number) => Promise<void>;
}

const MAX_MULTIPLIER = 5;

// ─── Component ────────────────────────────────────────────────────────────────

export function ManagePositionModal({
  position,
  onClose,
  onUpdate,
}: ManagePositionModalProps) {
  const [multiplier, setMultiplier] = useState(position.multiplier);

  const sliderPct = ((multiplier - 1) / (MAX_MULTIPLIER - 1)) * 100;

  // Net APY approximation (scales with multiplier)
  const baseAPY = position.netAPY / position.multiplier;
  const projectedAPY = baseAPY * multiplier;

  const liquidationRisk =
    multiplier < 2 ? "Low" : multiplier < 3.5 ? "Moderate" : "High";
  const riskColor =
    liquidationRisk === "Low"
      ? "text-success"
      : liquidationRisk === "Moderate"
      ? "text-warning"
      : "text-destructive";

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (onUpdate) await onUpdate(multiplier);
    onClose();
  }

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4 py-6"
    >
      <div
        className="w-full max-w-5xl rounded-2xl border border-surface-accent/15 bg-modal-bg overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-surface-accent/10 flex-shrink-0">
          <div className="flex items-center gap-3">
            <img
              src={position.icon}
              alt={position.asset}
              className="h-9 w-9 rounded-full ring-1 ring-surface-accent/20"
            />
            <div>
              <p className="text-sm font-semibold text-surface-foreground">
                Manage Position · {position.asset}
              </p>
              <p className="text-xs text-surface-foreground/35">
                Debt: {position.debtAsset} ·{" "}
                <span className="text-surface-accent">
                  {position.multiplier.toFixed(1)}× current
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

        {/* Body — two columns */}
        <div className="flex flex-col md:flex-row min-h-0">
          {/* ── Left: what is actually known about the position ──────────────
              There used to be a candlestick chart here, drawn by interpolating
              between `entryPrice` and `currentPrice` with sine noise. Both were
              hardcoded to 1 by the only caller, so the "price history" was a
              flat synthetic line presented as market data. No entry price is
              recorded on-chain and there is no price index behind this modal, so
              the chart could not be made real — only honest. */}
          <div className="flex-1 min-w-0 flex flex-col border-b md:border-b-0 md:border-r border-surface-accent/10">
            <div className="flex items-center px-4 pt-3 pb-2 flex-shrink-0">
              <span className="text-xs uppercase tracking-wider font-semibold text-surface-foreground/30">
                Price
              </span>
            </div>

            <div className="flex-1 min-h-[300px] md:min-h-[420px] flex flex-col items-center justify-center gap-3 px-6 text-center">
              <Info className="h-5 w-5 text-surface-foreground/25" />
              <p className="text-sm text-surface-foreground/50">
                No price history available
              </p>
              <p className="text-xs text-surface-foreground/30 max-w-xs">
                Positions do not record an entry price on-chain, and this build
                has no price index to reconstruct one from.
              </p>
              <div className="mt-2 flex items-center gap-1.5">
                <div className="w-5 border-t border-dashed border-destructive/70" />
                <span className="text-xs text-destructive/70">
                  Liquidation ${position.liqPrice.toFixed(2)}
                </span>
              </div>
            </div>
          </div>

          {/* ── Right: Controls ──────────────────────────────────────────────── */}
          <div className="w-full md:w-[340px] flex-shrink-0 flex flex-col px-5 pt-4 pb-5 gap-5 overflow-y-auto">
            {/* Position stats */}
            <div className="grid grid-cols-2 gap-2">
              <div className="rounded-xl border border-surface-accent/10 bg-surface-accent/[0.025] px-3 py-2.5">
                <p className="text-xs uppercase tracking-wider text-surface-foreground/30 mb-1">
                  Position
                </p>
                <p className="text-sm font-bold tabular-nums text-surface-foreground">
                  ${position.positionSize.toLocaleString()}
                </p>
              </div>
              <div className="rounded-xl border border-surface-accent/10 bg-surface-accent/[0.025] px-3 py-2.5">
                <p className="text-xs uppercase tracking-wider text-surface-foreground/30 mb-1">
                  Net APY
                </p>
                <p className="text-sm font-bold tabular-nums text-success">
                  {position.netAPY.toFixed(1)}%
                </p>
              </div>
              <div className="rounded-xl border border-surface-accent/10 bg-surface-accent/[0.025] px-3 py-2.5">
                <p className="text-xs uppercase tracking-wider text-surface-foreground/30 mb-1">
                  P&L
                </p>
                <p
                  className={cn(
                    "text-sm font-bold tabular-nums",
                    position.pnl === null
                      ? "text-surface-foreground/40"
                      : position.pnl >= 0
                      ? "text-success"
                      : "text-destructive",
                  )}
                >
                  {position.pnl === null
                    ? "—"
                    : `${position.pnl >= 0 ? "+" : ""}$${position.pnl.toFixed(0)}`}
                </p>
              </div>
              <div className="rounded-xl border border-surface-accent/10 bg-surface-accent/[0.025] px-3 py-2.5">
                <p className="text-xs uppercase tracking-wider text-surface-foreground/30 mb-1">
                  Current
                </p>
                <p className="text-sm font-bold tabular-nums text-surface-foreground">
                  {position.currentPrice === null
                    ? "—"
                    : `$${position.currentPrice.toFixed(2)}`}
                </p>
              </div>
            </div>

            {/* Multiplier slider */}
            <div className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                  <Zap className="h-3 w-3" />
                  Multiplier
                </span>
                <span className="text-sm font-bold tabular-nums text-surface-accent">
                  {multiplier.toFixed(1)}×
                </span>
              </div>

              <div className="relative">
                <input
                  type="range"
                  min={1}
                  max={MAX_MULTIPLIER}
                  step={0.1}
                  value={multiplier}
                  onChange={(e) => setMultiplier(parseFloat(e.target.value))}
                  className="w-full h-1.5 rounded-full appearance-none cursor-pointer bg-surface-accent/15 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-surface-accent [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-modal-bg [&::-webkit-slider-thumb]:shadow-lg [&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-surface-accent [&::-moz-range-thumb]:border-2 [&::-moz-range-thumb]:border-modal-bg"
                  // style-exception: slider gradient uses runtime sliderPct value
                  style={{
                    background: `linear-gradient(to right, rgba(198,152,229,0.6) ${sliderPct}%, rgba(198,152,229,0.12) ${sliderPct}%)`,
                  }}
                />
                <div className="flex justify-between mt-1">
                  {[1, 2, 3, 4, 5].map((m) => (
                    <button
                      key={m}
                      onClick={() => setMultiplier(m)}
                      className="text-xs text-surface-foreground/25 hover:text-surface-accent transition-colors cursor-pointer"
                    >
                      {m}×
                    </button>
                  ))}
                </div>
              </div>
            </div>

            {/* Projected stats */}
            <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
              <div className="flex items-center justify-between px-3.5 py-2.5">
                <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                  <Info className="h-3 w-3" />
                  Projected net APY
                </span>
                <span className="text-xs font-semibold text-success tabular-nums">
                  {projectedAPY.toFixed(1)}%
                </span>
              </div>
              <div className="flex items-center justify-between px-3.5 py-2.5">
                <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                  <AlertTriangle className="h-3 w-3" />
                  Liq. risk
                </span>
                <span className={cn("text-xs font-semibold", riskColor)}>
                  {liquidationRisk}
                </span>
              </div>
            </div>

            {/* Warning if multiplier changed significantly */}
            {/* {Math.abs(multiplier - position.multiplier) > 0.5 && (
              <div className="rounded-xl border border-warning/15 bg-warning/5 px-3.5 py-2.5 flex items-start gap-2">
                <AlertTriangle className="h-3.5 w-3.5 text-warning mt-0.5 flex-shrink-0" />
                <p className="text-xs text-warning/80 leading-relaxed">
                  Changing multiplier will rebalance debt. Review the
                  projected risk before confirming.
                </p>
              </div>
            )} */}

            <button
              onClick={handleSubmit}
              className="w-full rounded-xl py-3 text-sm font-semibold bg-surface-accent text-surface hover:bg-surface-accent/80 transition-all duration-200 active:scale-[0.98] cursor-pointer mt-auto"
            >
              Update Position
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

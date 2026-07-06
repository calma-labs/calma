import { type MultiplyStrategy } from "@/hooks/useMultiply";
import { formatUSD } from "@/lib/formatters";
import { cn } from "@/lib/utils";
import { ChevronRight } from "lucide-react";
import { useNavigate } from "react-router";

interface MultiplyTableProps {
  strategies: MultiplyStrategy[];
}

export function MultiplyTable({ strategies }: MultiplyTableProps) {
  const navigate = useNavigate();

  return (
    <div className="overflow-hidden rounded-xl border border-surface-accent/15">
      <div className="grid grid-cols-[minmax(0,2fr)_1fr_1fr_1fr_1fr_32px] gap-4 border-b border-surface-accent/10 bg-surface-accent/5 pl-5 pr-4 py-3">
        <span className="text-xs font-medium uppercase tracking-widest text-surface-foreground/30">
          Asset
        </span>
        <span className="text-xs font-medium uppercase tracking-widest text-surface-foreground/30">
          Debt Token
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-widest text-surface-foreground/30">
          Max Multiplier
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-widest text-surface-foreground/30">
          Max Net APY
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-widest text-surface-foreground/30">
          Market Size
        </span>
        <span />
      </div>

      {strategies.map((strategy, i) => (
        <div
          key={strategy.address}
          onClick={() => navigate(`/multiply/${strategy.address}`)}
          className={cn(
            "grid grid-cols-[minmax(0,2fr)_1fr_1fr_1fr_1fr_32px] gap-4 items-center pl-5 pr-4 py-5 transition-all duration-150 group cursor-pointer hover:bg-surface-accent/5",
            i < strategies.length - 1 && "border-b border-surface-accent/8",
          )}
        >
          <div className="flex items-center gap-3 min-w-0">
            <img
              src={strategy.lendIcon}
              alt={strategy.lendSymbol}
              width={36}
              height={36}
              className="h-9 w-9 flex-shrink-0 rounded-full object-contain ring-1 ring-surface-accent/20"
            />
            <div className="min-w-0">
              <p className="truncate text-sm font-medium text-surface-foreground group-hover:text-surface-accent transition-colors">
                {strategy.lendSymbol}
              </p>
              <p className="text-xs text-surface-foreground/35">{strategy.lendSymbol}</p>
            </div>
          </div>

          <div className="flex items-center gap-1.5 opacity-75">
            <img
              src={strategy.collateralIcon}
              alt={strategy.collateralSymbol}
              width={20}
              height={20}
              className="h-5 w-5 rounded-full ring-1 ring-surface-accent/15"
            />
            <span className="text-xs font-medium text-surface-foreground/40">
              {strategy.collateralSymbol}
            </span>
          </div>

          <div className="text-right">
            <span className="text-sm font-semibold text-surface-accent tabular-nums">
              {strategy.meta.maxMultiplier}×
            </span>
          </div>

          <div className="text-right">
            <p className="text-sm font-semibold text-success tabular-nums">
              {strategy.meta.maxNetAPY.toFixed(2)}%
            </p>
            <p className="text-xs text-surface-foreground/25 mt-0.5">net APY</p>
          </div>

          <div className="text-right">
            <p className="text-sm text-surface-foreground/80">
              {formatUSD(strategy.totalSupplied)}
            </p>
            <p className="text-xs text-surface-foreground/25 mt-0.5">supplied</p>
          </div>

          <ChevronRight className="h-4 w-4 text-surface-foreground/20 group-hover:text-surface-accent transition-colors" />
        </div>
      ))}
    </div>
  );
}

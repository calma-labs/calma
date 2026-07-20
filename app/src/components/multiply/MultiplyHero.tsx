import { ActionButton } from "@/components/common/ActionButton";
import { maxLeverageForLtv } from "@/hooks/useMultiply";
import type { Pool } from "@/types/pool";
import { Zap } from "lucide-react";

interface MultiplyHeroProps {
  pool: Pool;
  isWalletConnected: boolean;
  onOpenPosition: () => void;
}

export function MultiplyHero({
  pool,
  isWalletConnected,
  onOpenPosition,
}: MultiplyHeroProps) {
  const maxLeverage = maxLeverageForLtv(pool.account.ltv_percent);
  return (
    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-6 mb-8">
      <div className="flex items-center gap-4">
        <div className="relative flex-shrink-0">
          <img
            src={pool.lendIcon}
            alt={pool.lendSymbol}
            width={56}
            height={56}
            className="h-14 w-14 rounded-full ring-2 ring-surface-accent/20 object-contain"
          />
          {/* style-exception: 26px icon size required for stacked icon overlap design */}
          <img
            src={pool.collateralIcon}
            alt={pool.collateralSymbol}
            width={26}
            height={26}
            className="absolute -bottom-1 -right-1 h-[26px] w-[26px] rounded-full ring-2 ring-surface"
          />
        </div>

        <div>
          <div className="flex items-center gap-2.5 flex-wrap">
            <h1 className="text-2xl font-bold tracking-tight text-surface-foreground">
              {pool.name}
            </h1>
            <span className="text-sm font-medium text-surface-foreground/35">
              {pool.symbol} / {pool.collateralSymbol}
            </span>
          </div>
          <div className="flex items-center gap-2 mt-1.5">
            <span className="inline-flex items-center gap-1 rounded-md border border-surface-accent/20 bg-surface-accent/10 px-2 py-0.5 text-xs font-bold uppercase tracking-widest text-surface-accent">
              <Zap className="h-2.5 w-2.5" />
              Multiply
            </span>
            <span className="text-xs text-surface-foreground/30">
              Borrow {pool.lendSymbol} · loop collateral · up to {maxLeverage}×
            </span>
          </div>
        </div>
      </div>

      <ActionButton
        onClick={onOpenPosition}
        disabled={!isWalletConnected}
        variant="primary"
        icon={<Zap className="h-4 w-4" />}
        label={`Multiply ${pool.lendSymbol}`}
      />
    </div>
  );
}

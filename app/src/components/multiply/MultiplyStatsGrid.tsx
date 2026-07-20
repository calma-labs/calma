import { maxLeverageForLtv } from "@/hooks/useMultiply";
import { formatUSD } from "@/lib/formatters";
import { cn } from "@/lib/utils";
import type { Pool } from "@/types/pool";

interface StatCardProps {
  label: string;
  value: string;
  sub?: string;
  accent?: string;
}

function StatCard({ label, value, sub, accent }: StatCardProps) {
  return (
    <div className="flex flex-col gap-1.5 rounded-2xl border border-surface-accent/15 bg-surface-accent/[0.03] px-5 py-4">
      <p className="text-xs text-surface-foreground/35 uppercase tracking-widest font-medium">
        {label}
      </p>
      <p
        className={cn(
          "text-xl font-bold tabular-nums",
          accent ?? "text-surface-foreground",
        )}
      >
        {value}
      </p>
      {sub && <p className="text-xs text-surface-foreground/30">{sub}</p>}
    </div>
  );
}

interface MultiplyStatsGridProps {
  pool: Pool;
}

export function MultiplyStatsGrid({ pool }: MultiplyStatsGridProps) {
  const maxLeverage = maxLeverageForLtv(pool.account.ltv_percent);
  const maxNetAPY = pool.account.leveraged_net_apy(maxLeverage);

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
      <StatCard
        label="Max Multiplier"
        value={`${maxLeverage}×`}
        sub="leverage cap"
        accent="text-surface-accent"
      />
      <StatCard
        label="Max Net APY"
        value={`${maxNetAPY.toFixed(2)}%`}
        sub={`at ${maxLeverage}× leverage`}
        accent="text-success"
      />
      <StatCard
        label="Market Size"
        value={formatUSD(pool.totalSupplied)}
        sub="total supplied"
      />
      <StatCard
        label="Borrow APY"
        value={`${pool.borrowAPY.toFixed(2)}%`}
        sub={`${pool.lendSymbol} debt cost`}
        accent="text-warning"
      />
    </div>
  );
}

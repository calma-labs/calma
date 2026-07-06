import { MultiplyTable } from "@/components/multiply/MultiplyTable";
import { useMultiplyStrategies } from "@/hooks/useMultiply";
import { Search, Zap } from "lucide-react";
import { useMemo, useState } from "react";

export function MultiplyPage() {
  const { data: strategies = [], isLoading } = useMultiplyStrategies();
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return strategies
      .filter(
        (s) =>
          s.collateralSymbol.toLowerCase().includes(q) ||
          s.lendSymbol.toLowerCase().includes(q),
      )
      .sort((a, b) => b.meta.maxNetAPY - a.meta.maxNetAPY);
  }, [strategies, search]);

  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      <div className="mb-8">
        <div className="flex items-center gap-2.5 mb-2">
          <Zap className="h-5 w-5 text-surface-accent" />
          <h1 className="text-3xl font-semibold tracking-tight text-surface-foreground">
            Multiply
          </h1>
        </div>
        <p className="text-sm text-surface-foreground/50 max-w-lg">
          Leverage your position by looping collateral. Supply an asset, borrow
          the debt token and compound exposure in one click.
        </p>
      </div>

      <div className="mb-4">
        <div className="relative max-w-xs">
          <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-surface-foreground/30" />
          <input
            type="text"
            placeholder="Search strategies…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-accent/15 bg-surface-accent/5 py-1.5 pl-8 pr-3 text-xs text-surface-foreground placeholder:text-surface-foreground/25 transition-all focus:border-surface-accent/50 focus:bg-surface-accent/10 focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
      </div>

      {isLoading ? (
        <div className="flex items-center justify-center py-32 text-surface-foreground/30 text-sm">
          Loading strategies…
        </div>
      ) : (
        <MultiplyTable strategies={filtered} />
      )}
    </div>
  );
}

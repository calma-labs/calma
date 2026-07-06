import { formatRawTokens } from "@/lib/formatters";
import { cn } from "@/lib/utils";
import type { Pool } from "@/types/pool";
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
} from "lucide-react";
import { useNavigate } from "react-router";

export type SortKey =
  | "supplyAPY"
  | "borrowAPY"
  | "totalSupplied"
  | "totalBorrowed"
  | "ltv";
export type SortDir = "asc" | "desc";

interface MarketTableProps {
  pools: Pool[];
  sortKey: SortKey;
  sortDir: SortDir;
  onSort: (key: SortKey) => void;
}

function SortIcon({
  col,
  activeKey,
  sortDir,
}: {
  col: SortKey;
  activeKey: SortKey;
  sortDir: SortDir;
}) {
  if (activeKey !== col)
    return <ChevronsUpDown className="ml-1 inline-block h-3 w-3 opacity-30" />;
  return sortDir === "asc" ? (
    <ChevronUp className="ml-1 inline-block h-3 w-3 text-surface-accent" />
  ) : (
    <ChevronDown className="ml-1 inline-block h-3 w-3 text-surface-accent" />
  );
}

const CATEGORY_COLORS: Record<string, string> = {
  stablecoin: "text-success bg-success/8 border-success/20",
  volatile: "text-surface-accent bg-surface-accent/8 border-surface-accent/20",
  lsd: "text-warning bg-warning/8 border-warning/20",
};

const DATA_COLUMNS = [
  { key: "supplyAPY", label: "Supply APY" },
  { key: "borrowAPY", label: "Borrow APY" },
  { key: "totalSupplied", label: "Total Supplied" },
  { key: "totalBorrowed", label: "Total Borrowed" },
  { key: "ltv", label: "LTV" },
  { key: "collateral", label: "Collateral" },
] as const;

export function MarketTable({
  pools,
  sortKey,
  sortDir,
  onSort,
}: MarketTableProps) {
  const navigate = useNavigate();

  return (
    <div className="overflow-hidden rounded-2xl border border-surface-accent/12">
      {/* Header row */}
      <div className="grid grid-cols-[minmax(0,2fr)_1fr_repeat(5,1fr)_20px] gap-4 border-b border-surface-accent/10 bg-surface-accent/[0.03] px-5 py-3 items-center">
        <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30">
          Asset
        </span>
        <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30 text-right">
          Collateral
        </span>
        {DATA_COLUMNS.slice(0, 5).map(({ key, label }) => (
          <button
            key={key}
            onClick={() => onSort(key as SortKey)}
            className="flex cursor-pointer items-center justify-end text-xs font-semibold uppercase tracking-wider text-surface-foreground/30 hover:text-surface-accent transition-colors whitespace-nowrap"
          >
            {label}
            <SortIcon col={key as SortKey} activeKey={sortKey} sortDir={sortDir} />
          </button>
        ))}
        <span />
      </div>

      {pools.length === 0 ? (
        <div className="px-5 py-14 text-center text-sm text-surface-foreground/30">
          No assets match your search.
        </div>
      ) : (
        pools.map((pool, i) => (
          <div
            key={pool.id}
            onClick={() => navigate(`/pool/${pool.id}`)}
            className={cn(
              "grid grid-cols-[minmax(0,2fr)_1fr_repeat(5,1fr)_20px] gap-4 items-center px-5 py-4 cursor-pointer group transition-colors hover:bg-surface-accent/[0.04]",
              i < pools.length - 1 && "border-b border-surface-accent/8",
            )}
          >
            {/* Asset */}
            <div className="flex items-center gap-3 min-w-0">
              <img
                src={pool.icon}
                alt={pool.symbol}
                width={36}
                height={36}
                className="h-9 w-9 flex-shrink-0 rounded-full object-contain ring-1 ring-surface-accent/15"
              />
              <div className="min-w-0">
                <p className="truncate text-sm font-semibold text-surface-foreground group-hover:text-surface-accent transition-colors">
                  {pool.name}
                </p>
                <div className="flex items-center gap-1.5 mt-0.5">
                  <span className="text-xs text-surface-foreground/35">
                    {pool.symbol}
                  </span>
                  <span
                    className={cn(
                      "text-xs font-semibold px-1.5 py-px rounded border capitalize",
                      CATEGORY_COLORS[pool.category],
                    )}
                  >
                    {pool.category}
                  </span>
                </div>
              </div>
            </div>

            {/* Collateral */}
            <div className="text-right">
              {pool.collateralSymbol && (
                <div className="inline-flex items-center justify-end gap-1.5">
                  <span className="text-sm text-surface-foreground/75">
                    {pool.collateralSymbol}
                  </span>
                  {pool.collateralIcon && (
                    <img
                      src={pool.collateralIcon}
                      alt={pool.collateralSymbol}
                      width={20}
                      height={20}
                      className="h-5 w-5 rounded-full object-contain"
                    />
                  )}
                </div>
              )}
            </div>

            {/* Supply APY */}
            <div className="text-right">
              <p className="text-sm mr-2 font-semibold text-success tabular-nums">
                {pool.supplyAPY.toFixed(2)}%
              </p>
            </div>

            {/* Borrow APY */}
            <div className="text-right">
              <p className="text-sm mr-2 font-semibold text-destructive tabular-nums">
                {pool.borrowAPY.toFixed(2)}%
              </p>
            </div>

            {/* Total Supplied */}
            <div className="text-right">
              <p className="text-sm mr-2 tabular-nums text-surface-foreground/75">
                {formatRawTokens(pool.totalSupplied)}
              </p>
            </div>

            {/* Total Borrowed */}
            <div className="text-right">
              <p className="text-sm mr-2 tabular-nums text-surface-foreground/75">
                {formatRawTokens(pool.totalBorrowed)}
              </p>
            </div>

            {/* LTV */}
            <div className="text-right">
              <p className="text-sm mr-2 tabular-nums text-surface-foreground/75">
                {pool.account.ltv_percent}%
              </p>
            </div>

            <ChevronRight className="h-4 w-4 text-surface-foreground/20 group-hover:text-surface-accent transition-colors justify-self-end" />
          </div>
        ))
      )}
    </div>
  );
}

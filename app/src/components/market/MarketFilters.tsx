import { CATEGORY_FILTERS } from "@/constants/categories";
import { cn } from "@/lib/utils";
import type { Category } from "@/types/pool";
import { Search } from "lucide-react";

interface MarketFiltersProps {
  search: string;
  onSearchChange: (value: string) => void;
  categoryFilter: "all" | Category;
  onCategoryChange: (value: "all" | Category) => void;
}

export function MarketFilters({
  search,
  onSearchChange,
  categoryFilter,
  onCategoryChange,
}: MarketFiltersProps) {
  return (
    <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="relative flex-1 max-w-xs">
        <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-surface-foreground/30" />
        <input
          type="text"
          placeholder="Search assets…"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          className="w-full rounded-lg border border-surface-accent/15 bg-surface-accent/5 py-1.5 pl-8 pr-3 text-xs text-surface-foreground placeholder:text-surface-foreground/25 transition-all duration-200 focus:border-surface-accent/50 focus:bg-surface-accent/10 focus:ring-2 focus:ring-surface-accent/15 focus:scale-[1.01]"
        />
      </div>

      <div className="flex items-center gap-1.5">
        {CATEGORY_FILTERS.map(({ value, label }) => (
          <button
            key={value}
            onClick={() => onCategoryChange(value)}
            className={cn(
              "rounded-full cursor-pointer px-3 py-1 text-xs font-medium transition-all duration-150 active:scale-95",
              categoryFilter === value
                ? "bg-surface-accent text-surface shadow-[0_0_12px_rgba(198,152,229,0.25)]"
                : "border border-surface-accent/15 text-surface-foreground/40 hover:border-surface-accent/40 hover:text-surface-foreground/80",
            )}
          >
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}

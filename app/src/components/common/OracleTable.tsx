import {
  type FeedFreshness,
  type OraclePriceSide,
} from "@/hooks/program/useFeedFreshness";
import { useNow } from "@/hooks/useNow";
import { cn } from "@/lib/utils";
import { ChevronDown, Info } from "lucide-react";
import { useState } from "react";

/**
 * Collapsible on-chain vs. Hermes oracle price table with a freshness dot.
 * Presentational — pass `freshness` from `useFeedFreshness(pool)`. The consumer
 * typically also derives `freshness.willFail` to gate its action button.
 */
export function OracleTable({ freshness }: { freshness: FeedFreshness }) {
  const anyStale =
    freshness.onChain.collateral.stale ||
    freshness.onChain.lend.stale ||
    (freshness.hermes?.collateral.stale ?? false) ||
    (freshness.hermes?.lend.stale ?? false);
  const hermesStale =
    (freshness.hermes?.collateral.stale ?? false) ||
    (freshness.hermes?.lend.stale ?? false);
  const [open, setOpen] = useState(hermesStale);

  return (
    <div className="rounded-xl border border-surface-accent/10 overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center justify-between px-3.5 py-2.5 text-xs text-surface-foreground/50 hover:bg-surface-accent/5 transition-colors cursor-pointer"
      >
        <span className="flex items-center gap-1.5">
          <Info className="h-3 w-3" />
          Oracle prices
        </span>
        <span className="flex items-center gap-2">
          <span
            className={cn(
              "h-1.5 w-1.5 rounded-full",
              anyStale ? "bg-destructive" : "bg-success",
            )}
          />
          <ChevronDown
            className={cn(
              "h-3.5 w-3.5 transition-transform",
              open && "rotate-180",
            )}
          />
        </span>
      </button>
      {open && (
        <div className="grid grid-cols-[auto_1fr_1fr] text-xs border-t border-surface-accent/8">
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/40" />
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/50 font-medium">
            Collateral
          </div>
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/50 font-medium">
            Lend
          </div>

          <div className="border-t border-surface-accent/8 px-3 py-2 text-surface-foreground/40 flex flex-col justify-center">
            <span>On-chain</span>
            <span className="text-surface-foreground/30">
              max {freshness.onChain.maxAgeSecs}s
            </span>
          </div>
          <PriceCell side={freshness.onChain.collateral} />
          <PriceCell side={freshness.onChain.lend} />

          <div className="border-t border-surface-accent/8 px-3 py-2 text-surface-foreground/40 flex flex-col justify-center">
            <span>Hermes</span>
            <span className="text-surface-foreground/30">
              max {freshness.hermes?.maxAgeSecs ?? "—"}s
            </span>
          </div>
          <PriceCell side={freshness.hermes?.collateral} />
          <PriceCell side={freshness.hermes?.lend} />
        </div>
      )}
    </div>
  );
}

function PriceCell({ side }: { side: OraclePriceSide | undefined }) {
  const now = useNow();
  const stale = side?.stale ?? false;
  const ageSecs = side?.publishTs != null ? now - side.publishTs : null;
  return (
    <div
      className={cn(
        "border-t border-surface-accent/8 px-3 py-2 tabular-nums",
        stale ? "text-destructive" : "text-surface-foreground/70",
      )}
    >
      <div className="font-semibold">
        {side?.price != null
          ? `$${side.price.toLocaleString("en-US", {
              minimumFractionDigits: 2,
              maximumFractionDigits: 6,
            })}`
          : "—"}
      </div>
      <div className="text-surface-foreground/40">
        {ageSecs != null ? `${ageSecs}s` : "—"}
      </div>
    </div>
  );
}

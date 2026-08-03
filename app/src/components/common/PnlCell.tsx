import { cn } from "@/lib/utils";

/** Profit/loss for a position, or an em dash when it cannot be computed.
 *
 * `null` is not the same as zero here and must not be rendered as such: no entry
 * price is recorded on-chain, so for most positions PnL is unknown rather than
 * break-even. See the note on `MultiplyPosition.pnl`. */
export function PnlCell({ pnl, pct }: { pnl: number | null; pct: number | null }) {
  if (pnl === null || pct === null) {
    return (
      <span className="tabular-nums text-muted-foreground" title="No entry price is recorded on-chain, so PnL cannot be computed">
        —
      </span>
    );
  }

  const pos = pnl >= 0;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 tabular-nums",
        pos ? "text-success" : "text-destructive",
      )}
    >
      {pos ? "+" : ""}${pnl.toFixed(0)}{" "}
      <span className="font-normal text-xs opacity-70">
        ({pos ? "+" : ""}
        {pct.toFixed(2)}%)
      </span>
    </span>
  );
}

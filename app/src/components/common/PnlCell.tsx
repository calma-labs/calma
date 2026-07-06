import { cn } from "@/lib/utils";

export function PnlCell({ pnl, pct }: { pnl: number; pct: number }) {
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

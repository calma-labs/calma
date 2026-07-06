/** Shared badge components used across the portfolio feature */
import { cn } from "@/lib/utils";

export function HealthBadge({ value }: { value: number }) {
  const color =
    value <= 95
      ? "text-success bg-success/10"
      : value <= 99
      ? "text-warning bg-warning/10"
      : "text-destructive bg-destructive/10";
  return (
    <span
      className={cn(
        "px-2 py-0.5 rounded-full text-xs font-semibold tabular-nums",
        color,
      )}
    >
      {value}%
    </span>
  );
}

export function HFBadge({ value }: { value: number | null }) {
  if (value === null) {
    return (
      <span className="w-fit px-2 py-0.5 rounded-full text-xs font-semibold tabular-nums text-surface-foreground/30 bg-surface-foreground/5">
        N/A
      </span>
    );
  }
  const color =
    value >= 2.5
      ? "text-success bg-success/10"
      : value >= 1.5
      ? "text-warning bg-warning/10"
      : "text-destructive bg-destructive/10";
  return (
    <span
      className={cn(
        "w-fit px-2 py-0.5 rounded-full text-xs font-semibold tabular-nums",
        color,
      )}
    >
      {value.toFixed(2)}
    </span>
  );
}

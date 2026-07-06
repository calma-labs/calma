import { cn } from "@/lib/utils";

interface StatCardProps {
  label: string;
  value: string;
  sub?: string;
  positive?: boolean;
}

export function StatCard({ label, value, sub, positive }: StatCardProps) {
  return (
    <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.025] px-5 py-4">
      <p className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/35 mb-1">
        {label}
      </p>
      <p className="text-lg font-bold text-surface-foreground tabular-nums">{value}</p>
      {sub !== undefined && (
        <p
          className={cn(
            "text-xs mt-0.5 tabular-nums",
            positive === true
              ? "text-success"
              : positive === false
                ? "text-destructive"
                : "text-surface-foreground/35",
          )}
        >
          {sub}
        </p>
      )}
    </div>
  );
}

import { cn } from "@/lib/utils";

interface ChartRangePickerProps<T extends string> {
  options: readonly T[];
  value: T;
  onChange: (v: T) => void;
  activeClass?: string;
}

export function ChartRangePicker<T extends string>({
  options,
  value,
  onChange,
  activeClass = "bg-surface-accent/20 text-surface-accent",
}: ChartRangePickerProps<T>) {
  return (
    <div className="flex items-center gap-1">
      {options.map((r) => (
        <button
          key={r}
          onClick={() => onChange(r)}
          className={cn(
            "rounded-lg px-3 py-1.5 text-xs font-medium transition-all cursor-pointer",
            value === r
              ? activeClass
              : "text-surface-foreground/35 hover:text-surface-foreground/70",
          )}
        >
          {r}
        </button>
      ))}
    </div>
  );
}

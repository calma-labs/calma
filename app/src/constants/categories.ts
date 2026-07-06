import type { Category } from "@/types/pool";

export const CATEGORY_FILTERS: { value: "all" | Category; label: string }[] = [
  { value: "all", label: "All" },
  { value: "stablecoin", label: "Stablecoins" },
  { value: "volatile", label: "Volatile" },
  { value: "lsd", label: "LSD" },
];

export const CATEGORY_COLORS: Record<string, string> = {
  stablecoin: "text-success bg-success/8 border-success/20",
  volatile: "text-surface-accent bg-surface-accent/8 border-surface-accent/20",
  lsd: "text-warning bg-warning/8 border-warning/20",
};

export const CATEGORY_BADGE: Record<
  Category,
  { label: string; classes: string }
> = {
  stablecoin: {
    label: "Stablecoin",
    classes: "text-success bg-success/10 border-success/25",
  },
  volatile: {
    label: "Volatile",
    classes: "text-surface-accent bg-surface-accent/10 border-surface-accent/25",
  },
  lsd: {
    label: "LSD",
    classes: "text-warning bg-warning/10 border-warning/25",
  },
};

import { cn } from "@/lib/utils";

interface ActionButtonProps {
  label: string;
  icon: React.ReactNode;
  variant: "primary" | "secondary";
  disabled?: boolean;
  compact?: boolean;
  className?: string;
  title?: string;
  onClick?: () => void;
}

export function ActionButton({
  label,
  icon,
  variant,
  disabled,
  compact,
  className,
  title,
  onClick,
}: ActionButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title ?? (disabled ? "Connect wallet to continue" : undefined)}
      className={cn(
        "flex items-center gap-1.5 rounded-xl pl-1.5 py-1.5 font-medium transition-all duration-200",
        compact ? "text-xs pr-3" : "text-sm pr-4",
        "enabled:active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed",
        !disabled && "cursor-pointer",
        variant === "primary"
          ? "bg-surface-accent text-surface shadow-[0_0_20px_rgba(198,152,229,0.30)] enabled:hover:bg-surface-accent/85 enabled:hover:shadow-[0_0_28px_rgba(198,152,229,0.45)]"
          : "border border-surface-accent/25 bg-surface-accent/8 text-surface-accent enabled:hover:border-surface-accent/50 enabled:hover:bg-surface-accent/15",
      )}
    >
      <span
        className={cn(
          "flex items-center justify-center rounded-lg h-6 w-6",
          variant === "primary" ? "bg-surface/15" : "bg-surface-accent/15",
        )}
      >
        {icon}
      </span>
      {label}
    </button>
  );
}

interface PositionActionButtonProps {
  label: string;
  onClick?: () => void;
}

export function PositionActionButton({
  label,
  onClick,
}: PositionActionButtonProps) {
  return (
    <button
      onClick={onClick}
      className="rounded-lg cursor-pointer px-3 py-1 text-xs font-medium border border-surface-accent/20 text-surface-accent hover:bg-surface-accent/10 transition-colors whitespace-nowrap"
    >
      {label}
    </button>
  );
}

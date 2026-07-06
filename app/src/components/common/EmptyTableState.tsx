import { InboxIcon } from "lucide-react";

interface EmptyTableStateProps {
  message?: string;
}

export function EmptyTableState({
  message = "No Active Positions",
}: EmptyTableStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-16 gap-3 text-surface-foreground/20">
      <InboxIcon className="h-8 w-8" />
      <span className="text-sm font-medium">{message}</span>
    </div>
  );
}

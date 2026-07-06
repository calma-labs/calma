import { ArrowLeft } from "lucide-react";
import { useNavigate } from "react-router";

interface BackButtonProps {
  to: string;
  label: string;
}

export function BackButton({ to, label }: BackButtonProps) {
  const navigate = useNavigate();

  return (
    <button
      onClick={() => navigate(to)}
      className="group mb-8 flex items-center gap-1.5 text-xs font-medium text-surface-foreground/40 hover:text-surface-accent transition-colors cursor-pointer"
    >
      <ArrowLeft className="h-3.5 w-3.5 transition-transform group-hover:-translate-x-0.5" />
      {label}
    </button>
  );
}

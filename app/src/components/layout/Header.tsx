import { WalletConnectButton } from "@/components/wallet";
import { cn } from "@/lib/utils";
import { NavLink } from "react-router";

const NAV_LINKS = [
  { to: "/", end: true, label: "Market" },
  { to: "/multiply", end: false, label: "Multiply" },
  { to: "/portfolio", end: false, label: "Portfolio" },
  { to: "/pool/create", end: false, label: "Create Pool" },
] as const;

export function Header() {
  return (
    <header className="flex items-center justify-between border-b border-surface-accent/10 px-6 py-4 backdrop-blur-sm sticky top-0 z-10 bg-surface/80">
      <NavLink
        to="/"
        className="text-lg font-semibold tracking-tight text-surface-accent hover:text-surface-foreground transition-colors"
      >
        JBL
      </NavLink>

      <nav className="absolute left-1/2 -translate-x-1/2 flex items-center gap-1 p-1">
        {NAV_LINKS.map(({ to, end, label }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            className={({ isActive }) =>
              cn(
                "rounded-lg px-4 py-1.5 text-sm font-medium transition-all duration-150",
                isActive
                  ? "text-surface-accent"
                  : "text-surface-foreground/45 hover:text-surface-foreground/80",
              )
            }
          >
            {label}
          </NavLink>
        ))}
      </nav>

      <WalletConnectButton />
    </header>
  );
}

import { WalletConnectButton } from "@/components/wallet";
import { NavLink } from "react-router";

const NAV_LINKS = [
  { to: "/markets", end: true, label: "Market" },
  { to: "/multiply", end: false, label: "Multiply" },
  { to: "/portfolio", end: false, label: "Portfolio" },
  { to: "/pool/create", end: false, label: "Create Pool" },
] as const;

export function Header() {
  return (
    <header className="flex items-center justify-between border-b border-[#c698e5]/10 px-6 py-4 backdrop-blur-sm sticky top-0 z-10 bg-[#17081f]/80">
      <NavLink
        to="/markets"
        className="text-lg font-semibold tracking-tight text-[#c698e5] hover:text-[#efe0f7] transition-colors"
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
              `rounded-lg px-4 py-1.5 text-sm font-medium transition-all duration-150 ${
                isActive
                  ? "text-[#c698e5]"
                  : "text-[#efe0f7]/45 hover:text-[#efe0f7]/80"
              }`
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

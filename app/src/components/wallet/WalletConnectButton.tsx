import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { ChevronDown, LogOut, Wallet } from "lucide-react";
import { useCallback, useState } from "react";
import { FaucetMenu } from "./FaucetMenu";
import { WalletModal } from "./WalletModal";

function truncateAddress(address: string): string {
  if (address.length <= 8) return address;
  return `${address.slice(0, 4)}…${address.slice(-4)}`;
}

export function WalletConnectButton() {
  const { connected, connecting, disconnect, wallet } = useWalletConnection();
  const [modalOpen, setModalOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);

  const openModal = useCallback(() => setModalOpen(true), []);
  const closeModal = useCallback(() => setModalOpen(false), []);

  const handleDisconnect = useCallback(async () => {
    setMenuOpen(false);
    await disconnect();
  }, [disconnect]);

  // ── Connected state ──────────────────────────────────────────────────────────
  if (connected && wallet) {
    const address = String(wallet.account.address);

    return (
      <div className="relative">
        <button
          type="button"
          onClick={() => setMenuOpen((v) => !v)}
          // onBlur={() => setTimeout(() => setMenuOpen(false), 150)}
          className={cn(
            "flex items-center cursor-pointer gap-2 rounded-xl border border-surface-accent/25 bg-surface-accent/10 px-3 py-2",
            "text-xs font-medium text-surface-foreground transition-all",
            "hover:border-surface-accent/50 hover:bg-surface-accent/20",
            "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-0",
          )}
        >
          <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-surface-accent" />
          <span className="font-mono tracking-tight text-surface-foreground/80">
            {truncateAddress(address)}
          </span>
          <ChevronDown
            size={12}
            className={cn(
              "text-surface-accent/60 transition-transform duration-150",
              menuOpen && "rotate-180",
            )}
          />
        </button>

        {/* Dropdown */}
        {menuOpen && (
          <div
            className="absolute right-0 z-40 mt-2 w-44 rounded-xl border border-surface-accent/15 bg-wallet-subtle p-1 shadow-2xl"
            onMouseDown={(e) => e.preventDefault()} // Prevent blur from closing dropdown
          >
            <div className="mb-1 border-b border-surface-accent/10 px-3 py-2">
              <p className="text-xs uppercase tracking-widest text-surface-foreground/30">
                Connected
              </p>
              <p className="mt-0.5 font-mono text-xs text-surface-accent">
                {truncateAddress(address)}
              </p>
            </div>
            <FaucetMenu />
            <div className="mx-3 my-1 border-t border-surface-accent/10" />
            <button
              type="button"
              onClick={handleDisconnect}
              className={cn(
                "flex w-full cursor-pointer items-center gap-2 rounded-lg px-3 py-2",
                "text-xs font-medium text-destructive transition-colors",
                "hover:bg-destructive/10 focus-visible:ring-2 focus-visible:ring-ring",
              )}
            >
              <LogOut size={13} />
              Disconnect
            </button>
          </div>
        )}
      </div>
    );
  }

  // ── Disconnected state ───────────────────────────────────────────────────────
  return (
    <>
      <button
        type="button"
        onClick={openModal}
        disabled={connecting}
        className={cn(
          "flex cursor-pointer items-center gap-2 rounded-xl px-4 py-2",
          "bg-surface-accent text-xs font-semibold text-surface",
          "transition-all hover:bg-surface-accent/80 active:scale-95",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-0",
          "disabled:pointer-events-none disabled:opacity-60",
        )}
      >
        {connecting ? (
          <span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-surface/30 border-t-surface" />
        ) : (
          <Wallet size={13} />
        )}
        {connecting ? "Connecting…" : "Get Started"}
      </button>

      <WalletModal open={modalOpen} onClose={closeModal} />
    </>
  );
}

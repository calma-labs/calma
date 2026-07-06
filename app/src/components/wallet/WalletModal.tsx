import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { Download, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

// ── Featured wallets (always shown, icons hardcoded from wallet-standard) ──────

const FEATURED = [
  {
    key: "phantom",
    name: "Phantom",
    url: "https://phantom.app",
    icon: "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTA4IiBoZWlnaHQ9IjEwOCIgdmlld0JveD0iMCAwIDEwOCAxMDgiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CjxyZWN0IHdpZHRoPSIxMDgiIGhlaWdodD0iMTA4IiByeD0iMjYiIGZpbGw9IiNBQjlGRjIiLz4KPHBhdGggZmlsbC1ydWxlPSJldmVub2RkIiBjbGlwLXJ1bGU9ImV2ZW5vZGQiIGQ9Ik00Ni41MjY3IDY5LjkyMjlDNDIuMDA1NCA3Ni44NTA5IDM0LjQyOTIgODUuNjE4MiAyNC4zNDggODUuNjE4MkMxOS41ODI0IDg1LjYxODIgMTUgODMuNjU2MyAxNSA3NS4xMzQyQzE1IDUzLjQzMDUgNDQuNjMyNiAxOS44MzI3IDcyLjEyNjggMTkuODMyN0M4Ny43NjggMTkuODMyNyA5NCAzMC42ODQ2IDk0IDQzLjAwNzlDOTQgNTguODI1OCA4My43MzU1IDc2LjkxMjIgNzMuNTMyMSA3Ni45MTIyQzcwLjI5MzkgNzYuOTEyMiA2OC43MDUzIDc1LjEzNDIgNjguNzA1MyA3Mi4zMTRDNjguNzA1MyA3MS41NzgzIDY4LjgyNzUgNzAuNzgxMiA2OS4wNzE5IDY5LjkyMjlDNjUuNTg5MyA3NS44Njk5IDU4Ljg2ODUgODEuMzg3OCA1Mi41NzU0IDgxLjM4NzhDNDcuOTkzIDgxLjM4NzggNDUuNjcxMyA3OC41MDYzIDQ1LjY3MTMgNzQuNDU5OEM0NS42NzEzIDcyLjk4ODQgNDUuOTc2OCA3MS40NTU2IDQ2LjUyNjcgNjkuOTIyOVpNODMuNjc2MSA0Mi41Nzk0QzgzLjY3NjEgNDYuMTcwNCA4MS41NTc1IDQ3Ljk2NTggNzkuMTg3NSA0Ny45NjU4Qzc2Ljc4MTYgNDcuOTY1OCA3NC42OTg5IDQ2LjE3MDQgNzQuNjk4OSA0Mi41Nzk0Qzc0LjY5ODkgMzguOTg4NSA3Ni43ODE2IDM3LjE5MzEgNzkuMTg3NSAzNy4xOTMxQzgxLjU1NzUgMzcuMTkzMSA4My42NzYxIDM4Ljk4ODUgODMuNjc2MSA0Mi41Nzk0Wk03MC4yMTAzIDQyLjU3OTVDNzAuMjEwMyA0Ni4xNzA0IDY4LjA5MTYgNDcuOTY1OCA2NS43MjE2IDQ3Ljk2NThDNjMuMzE1NyA0Ny45NjU4IDYxLjIzMyA0Ni4xNzA0IDYxLjIzMyA0Mi41Nzk1QzYxLjIzMyAzOC45ODg1IDYzLjMxNTcgMzcuMTkzMSA2NS43MjE2IDM3LjE5MzFDNjguMDkxNiAzNy4xOTMxIDcwLjIxMDMgMzguOTg4NSA3MC4yMTAzIDQyLjU3OTVaIiBmaWxsPSIjRkZGREY4Ii8+Cjwvc3ZnPgo=",
  },
  {
    key: "metamask",
    name: "MetaMask",
    url: "https://metamask.io",
    icon: "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMzAiIGhlaWdodD0iMzAiIHZpZXdCb3g9IjAgMCAzMCAzMCIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48c3ZnIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgd2lkdGg9IjIzIiBoZWlnaHQ9IjIzIiB4PSIzLjUiIHk9IjMuNSIgdmlld0JveD0iMCAwIDE0MS41MSAxMzYuNDIiPjxwYXRoIGZpbGw9IiNGRjVDMTYiIGQ9Im0xMzIuMjQgMTMxLjc1LTMwLjQ4LTkuMDctMjIuOTkgMTMuNzQtMTYuMDMtLjAxLTIzLTEzLjc0LTMwLjQ3IDkuMDhMMCAxMDAuNDdsOS4yNy0zNC43M0wwIDM2LjQgOS4yNyAwbDQ3LjYgMjguNDRoMjcuNzZMMTMyLjI0IDBsOS4yNyAzNi4zOC05LjI3IDI5LjM2IDkuMjcgMzQuNzItOS4yNyAzMS4zWiIvPjxwYXRoIGZpbGw9IiNGRjVDMTYiIGQ9Im05LjI3IDAgNDcuNjEgMjguNDZMNTQuOTggNDggOS4yOSAwWm0zMC40NyAxMDAuNDggMjAuOTUgMTUuOTUtMjAuOTUgNi4yNHYtMjIuMlpNNTkuMDEgNzQuMSA1NSA0OCAyOS4yMiA2NS43NWgtLjAybC4wOCAxOC4yNyAxMC40NS05LjkyaDE5LjI5Wk0xMzIuMjUgMGwtNDcuNiAyOC40Nkw4Ni41MSA0OGw0NS43Mi00OFptLTMwLjQ3IDEwMC40OC0yMC45NCAxNS45NSAyMC45NCA2LjI0di0yMi4yWm0xMC41My0zNC43M0w4Ni41MyA0OCA4Mi41IDc0LjFoMTkuMjdsMTAuNDYgOS45LjA3LTE4LjI2WiIvPjxwYXRoIGZpbGw9IiNFMzQ4MDciIGQ9Im0zOS43MyAxMjIuNjctMzAuNDYgOS4wOEwwIDEwMC40OGgzOS43M3YyMi4yWk01OS4wMiA3NC4xbDUuODIgMzcuNzEtOC4wNy0yMC45Ny0yNy40OS02LjgyIDEwLjQ2LTkuOTJINTlabTQyLjc2IDQ4LjU5IDMwLjQ3IDkuMDcgOS4yNy0zMS4yN2gtMzkuNzR6TTgyLjUgNzQuMDlsLTUuODIgMzcuNzEgOC4wNi0yMC45NyAyNy41LTYuODItMTAuNDctOS45MnoiLz48cGF0aCBmaWxsPSIjRkY4RDVEIiBkPSJtMCAxMDAuNDcgOS4yNy0zNC43M0gyOS4ybC4wNyAxOC4yNyAyNy41IDYuODIgOC4wNiAyMC45Ny00LjE1IDQuNjItMjAuOTQtMTUuOTZIMFptMTQxLjUgMC05LjI2LTM0LjczaC0xOS45M2wtLjA3IDE4LjI3LTI3LjUgNi44Mi04LjA2IDIwLjk3IDQuMTUgNC42MiAyMC45NC0xNS45NmgzOS43NFpNODQuNjQgMjguNDRINTYuODhsLTEuODkgMTkuNTQgOS44NCA2My44aDExLjg1bDkuODUtNjMuOC0xLjktMTkuNTRaIi8+PHBhdGggZmlsbD0iIzY2MTgwMCIgZD0iTTkuMjcgMCAwIDM2LjM4bDkuMjcgMjkuMzZIMjkuMkw1NC45OCA0OHptNDMuOTggODEuNjdoLTkuMDNsLTQuOTIgNC44MSAxNy40NyA0LjMzLTMuNTItOS4xNVpNMTMyLjI0IDBsOS4yNyAzNi4zOC05LjI3IDI5LjM2aC0xOS45M0w4Ni41MyA0OHpNODguMjcgODEuNjdoOS4wNGw0LjkyIDQuODItMTcuNDkgNC4zNCAzLjUzLTkuMTdabS05LjUgNDIuMyAyLjA2LTcuNTQtNC4xNS00LjYySDY0LjgybC00LjE0IDQuNjIgMi4wNSA3LjU0Ii8+PHBhdGggZmlsbD0iI0MwQzRDRCIgZD0iTTc4Ljc3IDEyMy45N3YxMi40NUg2Mi43NHYtMTIuNDVoMTYuMDJaIi8+PHBhdGggZmlsbD0iI0U3RUJGNiIgZD0ibTM5Ljc0IDEyMi42NiAyMyAxMy43NnYtMTIuNDZsLTIuMDUtNy41NHptNjIuMDMgMC0yMyAxMy43NnYtMTIuNDZsMi4wNi03LjU0eiIvPjwvc3ZnPjwvc3ZnPg==",
  },
  {
    key: "backpack",
    name: "Backpack",
    url: "https://backpack.app",
    icon: "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI2NCIgaGVpZ2h0PSI2NCIgdmlld0JveD0iMCAwIDU1IDgwIiBmaWxsPSJub25lIj48cGF0aCBmaWxsLXJ1bGU9ImV2ZW5vZGQiIGNsaXAtcnVsZT0iZXZlbm9kZCIgZD0iTTMyLjcxIDYuMjkwMjZDMzUuNjE3OCA2LjI5MDI2IDM4LjM0NTIgNi42ODAwNSA0MC44NzA1IDcuNDAyOTZDMzguMzk4MiAxLjY0MDg1IDMzLjI2NDkgMCAyNy41NTE5IDBDMjEuODI3NyAwIDE2LjY4NTUgMS42NDcyOSAxNC4yMTg4IDcuNDM2OTJMMTYuNzI1NSA2LjY4ODU2IDE5LjQ0MTIgNi4yOTAyNiAyMi4zMzkgNi4yOTAyNkgzMi43MVpNMjEuNjczOSAxMi4wNzUyQzcuODY2NzcgMTIuMDc1MiAwIDIyLjkzNzEgMCAzNi4zMzZWNTAuMUMwIDUxLjQzOTkgMS4xMTkyOSA1Mi41IDIuNSA1Mi41SDUyLjVDNTMuODgwNyA1Mi41IDU1IDUxLjQzOTkgNTUgNTAuMVYzNi4zMzZDNTUgMjIuOTM3MSA0NS44NTIxIDEyLjA3NTIgMzIuMDQ0OSAxMi4wNzUySDIxLjY3MzlaTTI3LjQ4MDUgMzYuNDU1MUMzMi4zMTMgMzYuNDU1MSAzNi4yMzA1IDMyLjUzNzYgMzYuMjMwNSAyNy43MDUxQzM2LjIzMDUgMjIuODcyNiAzMi4zMTMgMTguOTU1MSAyNy40ODA1IDE4Ljk1NTFDMjIuNjQ4IDE4Ljk1NTEgMTguNzMwNSAyMi44NzI2IDE4LjczMDUgMjcuNzA1MUMxOC43MzA1IDMyLjUzNzYgMjIuNjQ4IDM2LjQ1NTEgMjcuNDgwNSAzNi40NTUxWk0wIDYwLjU5MDFDMCA1OS4yNTAzIDEuMTE5MjkgNTguMTY0MSAyLjUgNTguMTY0MUg1Mi41QzUzLjg4MDcgNTguMTY0MSA1NSA1OS4yNTAzIDU1IDYwLjU5MDFWNzUuMTQ2NkM1NSA3Ny44MjY0IDUyLjc2MTQgNzkuOTk4OCA1MCA3OS45OTg4SDVDMi4yMzg1NyA3OS45OTg4IDAgNzcuODI2NCAwIDc1LjE0NjZWNjAuNTkwMVoiIGZpbGw9IiNFMzNFM0YiLz48L3N2Zz4=",
  },
  {
    key: "solflare",
    name: "Solflare",
    url: "https://solflare.com",
    icon: "data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iVVRGLTgiPz48c3ZnIGlkPSJTIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MCA1MCI+PGRlZnM+PHN0eWxlPi5jbHMtMXtmaWxsOiMwMjA1MGE7c3Ryb2tlOiNmZmVmNDY7c3Ryb2tlLW1pdGVybGltaXQ6MTA7c3Ryb2tlLXdpZHRoOi41cHg7fS5jbHMtMntmaWxsOiNmZmVmNDY7fTwvc3R5bGU+PC9kZWZzPjxyZWN0IGNsYXNzPSJjbHMtMiIgeD0iMCIgd2lkdGg9IjUwIiBoZWlnaHQ9IjUwIiByeD0iMTIiIHJ5PSIxMiIvPjxwYXRoIGNsYXNzPSJjbHMtMSIgZD0iTTI0LjIzLDI2LjQybDIuNDYtMi4zOCw0LjU5LDEuNWMzLjAxLDEsNC41MSwyLjg0LDQuNTEsNS40MywwLDEuOTYtLjc1LDMuMjYtMi4yNSw0LjkzbC0uNDYuNS4xNy0xLjE3Yy42Ny00LjI2LS41OC02LjA5LTQuNzItNy40M2wtNC4zLTEuMzhoMFpNMTguMDUsMTEuODVsMTIuNTIsNC4xNy0yLjcxLDIuNTktNi41MS0yLjE3Yy0yLjI1LS43NS0zLjAxLTEuOTYtMy4zLTQuNTF2LS4wOGgwWk0xNy4zLDMzLjA2bDIuODQtMi43MSw1LjM0LDEuNzVjMi44LjkyLDMuNzYsMi4xMywzLjQ2LDUuMThsLTExLjY1LTQuMjJoMFpNMTMuNzEsMjAuOTVjMC0uNzkuNDItMS41NCwxLjEzLTIuMTcuNzUsMS4wOSwyLjA1LDIuMDUsNC4wOSwyLjcxbDQuNDIsMS40Ni0yLjQ2LDIuMzgtNC4zNC0xLjQyYy0yLS42Ny0yLjg0LTEuNjctMi44NC0yLjk2TTI2LjgyLDQyLjg3YzkuMTgtNi4wOSwxNC4xMS0xMC4yMywxNC4xMS0xNS4zMiwwLTMuMzgtMi01LjI2LTYuNDMtNi43MmwtMy4zNC0xLjEzLDkuMTQtOC43Ny0xLjg0LTEuOTYtMi43MSwyLjM4LTEyLjgxLTQuMjJjLTMuOTcsMS4yOS04Ljk3LDUuMDktOC45Nyw4Ljg5LDAsLjQyLjA0LjgzLjE3LDEuMjktMy4zLDEuODgtNC42MywzLjYzLTQuNjMsNS44LDAsMi4wNSwxLjA5LDQuMDksNC41NSw1LjIybDIuNzUuOTItOS41Miw5LjE0LDEuODQsMS45NiwyLjk2LTIuNzEsMTQuNzMsNS4yMmgwWiIvPjwvc3ZnPg==",
  },
] as const;

function matchesKey(connectorName: string, key: string) {
  return connectorName.toLowerCase().replace(/\s+/g, "").includes(key);
}

// ── WalletIcon ─────────────────────────────────────────────────────────────────

function WalletIcon({ icon, name }: { icon?: string; name: string }) {
  if (icon) {
    return (
      <img
        src={icon}
        alt={name}
        width={28}
        height={28}
        className="h-7 w-7 rounded-lg object-contain"
      />
    );
  }
  return (
    <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-surface-accent/15 text-xs font-bold text-surface-accent">
      {name[0]?.toUpperCase() ?? "?"}
    </span>
  );
}

// ── WalletRow (connected connector) ───────────────────────────────────────────

interface WalletRowProps {
  id: string;
  name: string;
  icon?: string;
  detected: boolean;
  onConnect: (id: string) => void;
  connecting: boolean;
}

function WalletRow({
  id,
  name,
  icon,
  detected,
  onConnect,
  connecting,
}: WalletRowProps) {
  return (
    <button
      type="button"
      onClick={() => onConnect(id)}
      disabled={connecting}
      className={cn(
        "group flex w-full cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5",
        "text-left transition-colors duration-100",
        "hover:bg-surface-accent/10",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-0",
        "disabled:pointer-events-none disabled:opacity-50",
      )}
    >
      <WalletIcon icon={icon} name={name} />
      <span className="flex-1 text-sm font-medium text-surface-foreground">
        {name}
      </span>
      {connecting ? (
        <span className="h-3 w-3 animate-spin rounded-full border-2 border-surface-accent/20 border-t-surface-accent" />
      ) : detected ? (
        <div className="flex gap-1 items-center">
          <span className="h-1.5 w-1.5 rounded-full bg-surface-accent" />
          <span className="text-xs font-medium text-surface-accent/70">
            Detected
          </span>
        </div>
      ) : null}
    </button>
  );
}

// ── WalletInstallRow (not installed) ──────────────────────────────────────────

function WalletInstallRow({
  name,
  url,
  icon,
}: {
  name: string;
  url: string;
  icon: string;
}) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      className={cn(
        "group flex w-full cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5",
        "transition-colors duration-100 hover:bg-surface-accent/5",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-0",
      )}
    >
      <img
        src={icon}
        alt={name}
        width={28}
        height={28}
        className="h-7 w-7 rounded-lg object-contain opacity-30 grayscale"
      />
      <span className="flex-1 text-sm font-medium text-surface-foreground/30">
        {name}
      </span>
      <span className="flex items-center gap-1 text-xs font-medium text-surface-foreground/20 transition-colors group-hover:text-surface-accent/60">
        <Download size={11} />
        Install
      </span>
    </a>
  );
}

// ── WalletModal ────────────────────────────────────────────────────────────────

interface WalletModalProps {
  open: boolean;
  onClose: () => void;
}

export function WalletModal({ open, onClose }: WalletModalProps) {
  const { connecting, connectors, connect } = useWalletConnection();
  const overlayRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  useEffect(() => {
    document.body.style.overflow = open ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  async function handleConnect(id: string) {
    await connect(id);
    onClose();
  }

  if (!open) return null;

  // Build ordered list: featured first, then any remaining detected connectors
  const featuredRows = FEATURED.map((f) => {
    const connector = connectors.find((c) => matchesKey(c.name, f.key));
    return { ...f, connector };
  });

  const extraConnectors = connectors.filter(
    (c) => !FEATURED.some((f) => matchesKey(c.name, f.key)),
  );

  return createPortal(
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-label="Connect wallet"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === overlayRef.current) onClose();
      }}
    >
      {/* style-exception: wallet modal panel needs fixed max-width to prevent overflow on narrow screens */}
      <div className="relative w-full max-w-sm space-y-3 rounded-3xl border border-surface-accent/15 bg-wallet-bg shadow-2xl">
        {/* Close */}
        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className="absolute right-4 top-4 z-10 cursor-pointer rounded-full p-1.5 text-surface-foreground/25 transition-colors hover:bg-surface-accent/10 hover:text-surface-accent focus-visible:ring-2 focus-visible:ring-ring"
        >
          <X size={16} />
        </button>

        {/* Header */}
        <div className="px-6 text-center pb-4 pt-7">
          <p className="text-base font-semibold text-surface-foreground">
            Connect wallet
          </p>
          <p className="mt-0.5 text-xs text-surface-foreground/35">
            Select a wallet to continue
          </p>
        </div>

        {/* Wallet list */}
        <div className="flex flex-col gap-0.5 px-3">
          {featuredRows.map((f) =>
            f.connector ? (
              <WalletRow
                key={f.connector.id}
                id={f.connector.id}
                name={f.connector.name}
                icon={f.connector.icon}
                detected={!!f.connector.ready}
                onConnect={handleConnect}
                connecting={connecting}
              />
            ) : (
              <WalletInstallRow
                key={f.key}
                name={f.name}
                url={f.url}
                icon={f.icon}
              />
            ),
          )}

          {extraConnectors.length > 0 && (
            <>
              <div className="my-1.5 h-px bg-surface-accent/10" />
              {extraConnectors.map((c) => (
                <WalletRow
                  key={c.id}
                  id={c.id}
                  name={c.name}
                  icon={c.icon}
                  detected={!!c.ready}
                  onConnect={handleConnect}
                  connecting={connecting}
                />
              ))}
            </>
          )}
        </div>

        {/* Footer — no divider, just soft spacing */}
        <p className="pb-5 pt-2 text-center text-xs text-surface-foreground/25 px-6">
          By connecting you agree to the{" "}
          <span className="cursor-pointer text-surface-accent/60 underline underline-offset-2 hover:text-surface-accent">
            Terms of Service
          </span>
        </p>
      </div>
    </div>,
    document.body,
  );
}

import { useCreateMint } from "@/hooks/program/useCreateMint";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { Keypair } from "@solana/web3.js";
import { CheckCircle2, Copy, Loader2, Plus, RefreshCw, ShieldAlert, ShieldCheck } from "lucide-react";
import { useState } from "react";

const ADMIN_WALLET = "8vW1zRVXWqCq5z5xZGDVKmosg5PBX1yEQZAeffewEtkX";

// ─── guard ────────────────────────────────────────────────────────────────────

export function AdminGuard({ children }: { children: React.ReactNode }) {
  const { connected, wallet } = useWalletConnection();

  const isAdmin =
    connected && wallet && String(wallet.account.address) === ADMIN_WALLET;

  if (!isAdmin) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh] gap-4 text-center px-4">
        <span className="flex h-14 w-14 items-center justify-center rounded-2xl bg-[#d45677]/15">
          <ShieldAlert className="h-7 w-7 text-[#d45677]" />
        </span>
        <div>
          <h1 className="text-2xl font-semibold text-[#efe0f7]">
            Access Denied
          </h1>
          <p className="text-sm text-[#efe0f7]/50 mt-1">
            {connected
              ? "Your wallet is not authorized to view this page."
              : "Connect the admin wallet to access this page."}
          </p>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}

// ─── create mint tool ─────────────────────────────────────────────────────────

function CreateMintTool() {
  const [decimals, setDecimals] = useState("6");
  const [keypair, setKeypair] = useState<Keypair>(() => Keypair.generate());
  const [confirmed, setConfirmed] = useState(false);
  const { mutateAsync, isPending, error } = useCreateMint();

  const mintAddress = keypair.publicKey.toBase58();

  const decimalsNum = parseInt(decimals, 10);
  const decimalsValid =
    decimals !== "" && !isNaN(decimalsNum) && decimalsNum >= 0 && decimalsNum <= 18;

  function regenerate() {
    setKeypair(Keypair.generate());
    setConfirmed(false);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!decimalsValid) return;
    setConfirmed(false);
    await mutateAsync({ decimals: decimalsNum, mintKeypair: keypair });
    setConfirmed(true);
  }

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          <Plus className="h-4 w-4" />
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">Create Mint</h2>
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        Creates a new SPL token mint with <span className="font-mono">MINTER_KEYPAIR</span> as the
        mint authority.
      </p>

      <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-4">
        {/* Mint address — shown before and after */}
        <div
          className={cn(
            "rounded-xl border px-4 py-3 flex items-start gap-3 transition-colors duration-300",
            confirmed
              ? "border-[#34d399]/25 bg-[#34d399]/5"
              : "border-[#c698e5]/15 bg-[#c698e5]/[0.03]",
          )}
        >
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <p
                className={cn(
                  "text-[10px] uppercase tracking-wider",
                  confirmed ? "text-[#34d399]/60" : "text-[#efe0f7]/30",
                )}
              >
                Mint Address
              </p>
              {confirmed && (
                <CheckCircle2 className="h-3 w-3 text-[#34d399]" />
              )}
            </div>
            <p className="font-mono text-xs text-[#efe0f7]/80 break-all">{mintAddress}</p>
          </div>
          <div className="flex shrink-0 gap-1">
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(mintAddress)}
              title="Copy address"
              className="rounded-lg p-1.5 text-[#efe0f7]/40 hover:bg-[#c698e5]/10 hover:text-[#c698e5] transition-colors cursor-pointer"
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
            {!confirmed && (
              <button
                type="button"
                onClick={regenerate}
                title="Generate new address"
                disabled={isPending}
                className="rounded-lg p-1.5 text-[#efe0f7]/40 hover:bg-[#c698e5]/10 hover:text-[#c698e5] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <label
            htmlFor="decimals"
            className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45"
          >
            Decimals
          </label>
          <input
            id="decimals"
            type="number"
            min={0}
            max={18}
            step={1}
            value={decimals}
            onChange={(e) => { setDecimals(e.target.value); setConfirmed(false); }}
            placeholder="6"
            className={cn(
              "w-full rounded-xl border bg-[#c698e5]/[0.04] px-4 py-2.5 text-sm text-[#efe0f7]",
              "placeholder:text-[#efe0f7]/25 outline-none transition-colors duration-150",
              "focus:bg-[#c698e5]/[0.07]",
              decimalsValid || decimals === ""
                ? "border-[#c698e5]/20 hover:border-[#c698e5]/35 focus:border-[#c698e5]/50"
                : "border-[#d45677]/60 focus:border-[#d45677]",
            )}
          />
          {!decimalsValid && decimals !== "" && (
            <p className="text-[11px] text-[#d45677]">Must be between 0 and 18</p>
          )}
        </div>

        {error && (
          <p className="text-[11px] text-[#d45677]">
            {error instanceof Error ? error.message : "Unknown error"}
          </p>
        )}

        <div className="flex items-center justify-between pt-1">
          {confirmed ? (
            <button
              type="button"
              onClick={regenerate}
              className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40 hover:text-[#efe0f7]/70 transition-colors cursor-pointer"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              Create another
            </button>
          ) : (
            <span />
          )}
          <button
            type="submit"
            disabled={isPending || !decimalsValid || confirmed}
            className={cn(
              "flex items-center gap-2 rounded-xl px-6 py-2.5 text-sm font-semibold transition-all duration-200",
              "bg-[#c698e5] text-[#17081f] shadow-[0_0_20px_rgba(198,152,229,0.30)]",
              "enabled:hover:bg-[#d4aeee] enabled:hover:shadow-[0_0_28px_rgba(198,152,229,0.45)]",
              "enabled:active:scale-95 cursor-pointer",
              "disabled:opacity-50 disabled:cursor-not-allowed",
            )}
          >
            {isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Creating…
              </>
            ) : confirmed ? (
              <>
                <CheckCircle2 className="h-4 w-4" />
                Created
              </>
            ) : (
              <>
                <Plus className="h-4 w-4" />
                Create Mint
              </>
            )}
          </button>
        </div>
      </form>
    </div>
  );
}

// ─── page ─────────────────────────────────────────────────────────────────────

export function AdminPage() {
  return (
    <AdminGuard>
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <div className="flex items-center gap-3 mb-8">
          <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-[#c698e5]/15">
            <ShieldCheck className="h-5 w-5 text-[#c698e5]" />
          </span>
          <h1 className="text-3xl font-semibold tracking-tight text-[#efe0f7]">
            Admin
          </h1>
        </div>

        <div className="w-2/3 flex flex-col gap-5">
          <CreateMintTool />
        </div>
      </div>
    </AdminGuard>
  );
}

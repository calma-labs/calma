import { useValidLendingAccounts } from "@/hooks/program/useValidLendingAccounts";
import { useFaucet, useFaucetAll } from "@/hooks/useFaucet";
import { getPoolMeta } from "@/config/poolRegistry";
import { cn } from "@/lib/utils";
import { PublicKey } from "@solana/web3.js";
import { Droplets, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

interface MintEntry {
  mint: PublicKey;
  symbol: string;
  icon: string;
}

function FaucetRow({ entry }: { entry: MintEntry }) {
  const { mutate, isPending, error } = useFaucet(entry.mint);

  const handleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    mutate();
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={isPending}
      className={cn(
        "flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2",
        "text-xs font-medium transition-colors",
        "text-surface-accent/80 hover:bg-surface-accent/10 cursor-pointer",
        isPending && "opacity-50",
      )}
    >
      <div className="flex items-center gap-2">
        <img src={entry.icon} alt={entry.symbol} className="h-4 w-4 rounded-full" />
        <span className="text-surface-foreground/70">{entry.symbol}</span>
      </div>
      <div className="flex items-center gap-2">
        {error && <span className="text-xs text-destructive">Failed</span>}
        {isPending ? (
          <Loader2 size={11} className="shrink-0 animate-spin text-surface-accent" />
        ) : (
          <Droplets size={11} className="shrink-0 text-surface-accent/60" />
        )}
      </div>
    </button>
  );
}

/**
 * Renders a faucet section inside the wallet dropdown.
 * Shows individual mint rows plus a "Mint all" button that sends a single tx.
 */
export function FaucetMenu() {
  const { data: pools, isLoading } = useValidLendingAccounts();
  const [validEntries, setValidEntries] = useState<MintEntry[]>([]);
  const validMints = validEntries.map((e) => e.mint);
  const { mutate: mintAll, isPending: mintAllPending, error: mintAllError } = useFaucetAll(validMints);

  useEffect(() => {
    if (!pools || pools.length === 0) {
      setValidEntries([]);
      return;
    }

    // Pools are already validated — just build mint → {symbol, icon} from registry
    const mintMeta = new Map<string, { mint: PublicKey; symbol: string; icon: string }>();
    for (const p of pools) {
      const meta = getPoolMeta(p.publicKey.toBase58());
      mintMeta.set(new PublicKey(p.account.collateral_mint).toBase58(), {
        mint: new PublicKey(p.account.collateral_mint),
        symbol: meta.collateralSymbol,
        icon: meta.collateralIcon,
      });
      mintMeta.set(new PublicKey(p.account.lend_mint).toBase58(), {
        mint: new PublicKey(p.account.lend_mint),
        symbol: meta.lendSymbol,
        icon: meta.lendIcon,
      });
    }

    setValidEntries(Array.from(mintMeta.values()));
  }, [pools]);

  if (isLoading) {
    return (
      <div className="flex items-center gap-1.5 px-3 py-2 text-xs text-surface-foreground/30">
        <Loader2 size={10} className="animate-spin" />
        Loading pools…
      </div>
    );
  }

  if (validEntries.length === 0) return null;

  const handleMintAll = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    mintAll();
  };

  return (
    <>
      <div className="mx-3 my-1 border-t border-surface-accent/10" />
      <div className="flex items-center justify-between px-3 py-1.5">
        <p className="text-xs uppercase tracking-widest text-surface-foreground/30">Faucet</p>
        <button
          type="button"
          onClick={handleMintAll}
          disabled={mintAllPending}
          className={cn(
            "flex items-center gap-1 rounded px-2 py-0.5 text-xs font-medium transition-colors",
            "text-surface-accent/70 hover:bg-surface-accent/10 cursor-pointer",
            mintAllPending && "opacity-50",
          )}
        >
          {mintAllPending ? (
            <Loader2 size={9} className="animate-spin" />
          ) : (
            <Droplets size={9} />
          )}
          Mint all
          {mintAllError && <span className="ml-1 text-destructive">Failed</span>}
        </button>
      </div>
      {validEntries.map((entry) => (
        <FaucetRow key={entry.mint.toBase58()} entry={entry} />
      ))}
    </>
  );
}

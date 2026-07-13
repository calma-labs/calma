import { useParticipate } from "@/hooks/program/useParticipate";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { useTokenBalance } from "@/hooks/useWalletBalances";
import { cn } from "@/lib/utils";
import type { PoolWithIrm } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { Info, Layers, Loader2, Wallet, X } from "lucide-react";
import { useState } from "react";

interface ParticipateModalProps {
  pool: Pool;
  poolData: PoolWithIrm;
  onClose: () => void;
}

export function ParticipateModal({
  pool,
  poolData,
  onClose,
}: ParticipateModalProps) {
  const [amount, setAmount] = useState("");
  const { wallet } = useWalletConnection();

  const { data: lendDecimals } = useMintDecimals(new PublicKey(poolData.lend_mint));
  const lendBalance = useTokenBalance(new PublicKey(poolData.lend_mint));

  const displaySymbol = pool.lendSymbol;
  const displayIcon = pool.lendIcon;

  const participateMutation = useParticipate();
  const isPending = participateMutation.isPending;

  const limit = lendBalance?.uiAmount ?? 0;

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    const numAmount = parseFloat(amount);
    if (!numAmount || numAmount <= 0 || !wallet) return;

    const authority = new PublicKey(wallet.account.publicKey);
    const decimals = lendDecimals ?? 6;
    const rawAmount = new BN(Math.floor(numAmount * 10 ** decimals));
    const userLendTokenAccount = getAssociatedTokenAddressSync(
      new PublicKey(poolData.lend_mint),
      authority,
    );

    await participateMutation.mutateAsync({
      pool: new PublicKey(pool.address),
      lendMint: new PublicKey(poolData.lend_mint),
      userLendTokenAccount,
      amount: rawAmount,
    });

    onClose();
  }

  const canSubmit =
    !!amount && parseFloat(amount) > 0 && parseFloat(amount) <= limit && limit > 0 && !isPending && !!wallet;

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-sm rounded-2xl border border-surface-accent/15 bg-modal-bg overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4">
          <p className="text-base font-semibold text-surface-foreground">Lend</p>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-surface-foreground/30 hover:bg-surface-accent/10 hover:text-surface-foreground transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 flex flex-col gap-4">
          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between gap-2 px-2">
              <span className="text-xs text-surface-foreground/40 flex-shrink-0">
                Amount
              </span>
              <button
                onClick={() => setAmount(limit.toFixed(6))}
                className="flex items-center gap-1 text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer min-w-0"
              >
                <Wallet className="h-3 w-3 flex-shrink-0" />
                <span className="flex-shrink-0">Balance:</span>
                <span className="tabular-nums text-surface-foreground/55 truncate">
                  {limit.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 6,
                  })}{" "}
                  {displaySymbol}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-surface-accent/15 bg-surface-accent/5 px-3.5 py-3 transition-colors focus-within:border-surface-accent/40">
              <img
                src={displayIcon}
                alt={displaySymbol}
                width={20}
                height={20}
                className="h-5 w-5 rounded-full flex-shrink-0"
              />
              <input
                type="number"
                min="0"
                placeholder="0.00"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-surface-foreground placeholder-surface-foreground/20 tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-xs font-semibold text-surface-foreground/40 flex-shrink-0">
                {displaySymbol}
              </span>
            </div>
          </div>

          {/* Percentage shortcuts */}
          <div className="flex gap-1.5">
            {[25, 50, 75, 100].map((p) => (
              <button
                key={p}
                onClick={() => setAmount(((limit * p) / 100).toFixed(6))}
                className="flex-1 rounded-lg border border-surface-accent/15 py-1.5 text-xs font-medium text-surface-foreground/35 hover:border-surface-accent/35 hover:text-surface-accent transition-all cursor-pointer"
              >
                {p}%
              </button>
            ))}
          </div>

          {/* Info rows */}
          <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Supply APY
              </span>
              <span className="text-xs font-semibold text-success">
                {pool.supplyAPY.toFixed(2)}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Layers className="h-3 w-3" />
                You receive
              </span>
              <span className="text-xs font-semibold text-surface-accent">
                LP tokens
              </span>
            </div>
          </div>

          {/* Submit */}
          <button
            disabled={!canSubmit}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] flex items-center justify-center gap-2",
              canSubmit
                ? "bg-surface-accent text-surface hover:bg-surface-accent/80 cursor-pointer"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            Lend {displaySymbol}
          </button>
        </div>
      </div>
    </div>
  );
}

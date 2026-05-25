import { useDeposit } from "@/hooks/program/useDeposit";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { useTokenBalance } from "@/hooks/useWalletBalances";
import { cn } from "@/lib/utils";
import type { PoolAccount } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { Info, Loader2, Wallet, X } from "lucide-react";
import { useState } from "react";

interface DepositModalProps {
  pool: Pool;
  poolData: PoolAccount;
  onClose: () => void;
}

export function DepositModal({ pool, poolData, onClose }: DepositModalProps) {
  const [amount, setAmount] = useState("");
  const { wallet } = useWalletConnection();

  const collateralBalance = useTokenBalance(new PublicKey(poolData.collateral_mint));
  const { data: collateralDecimals } = useMintDecimals(new PublicKey(poolData.collateral_mint));

  const depositMutation = useDeposit();
  const isPending = depositMutation.isPending;

  const limit = collateralBalance?.uiAmount ?? 0;

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    const numAmount = parseFloat(amount);
    if (!numAmount || numAmount <= 0 || !wallet) return;

    const authority = new PublicKey(wallet.account.publicKey);
    const decimals = collateralDecimals ?? 9;
    const rawAmount = new BN(Math.floor(numAmount * 10 ** decimals));
    const userTokenAccount = getAssociatedTokenAddressSync(
      new PublicKey(poolData.collateral_mint),
      authority,
    );

    await depositMutation.mutateAsync({
      pool: new PublicKey(pool.address),
      collateralMint: new PublicKey(poolData.collateral_mint),
      userTokenAccount,
      amount: rawAmount,
    });

    onClose();
  }

  const canSubmit =
    !!amount && parseFloat(amount) > 0 && !isPending && !!wallet;

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-sm rounded-2xl border border-[#c698e5]/15 bg-[#1a0d24] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4">
          <p className="text-base font-semibold text-[#efe0f7]">Deposit</p>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-[#efe0f7]/30 hover:bg-[#c698e5]/10 hover:text-[#efe0f7] transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 flex flex-col gap-4">
          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between gap-2 px-2">
              <span className="text-[11px] text-[#efe0f7]/40 flex-shrink-0">
                Amount
              </span>
              <button
                onClick={() => setAmount(String(limit))}
                className="flex items-center gap-1 text-[11px] text-[#efe0f7]/35 hover:text-[#c698e5] transition-colors cursor-pointer min-w-0"
              >
                <Wallet className="h-3 w-3 flex-shrink-0" />
                <span className="flex-shrink-0">Balance:</span>
                <span className="tabular-nums text-[#efe0f7]/55 truncate">
                  {limit.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 6,
                  })}{" "}
                  {pool.collateralSymbol}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/5 px-3.5 py-3 transition-colors focus-within:border-[#c698e5]/40">
              <img
                src={pool.collateralIcon}
                alt={pool.collateralSymbol}
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
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-[#efe0f7] placeholder-[#efe0f7]/20 outline-none tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-[11px] font-semibold text-[#efe0f7]/40 flex-shrink-0">
                {pool.collateralSymbol}
              </span>
            </div>
          </div>

          {/* Percentage shortcuts */}
          <div className="flex gap-1.5">
            {[25, 50, 75, 100].map((p) => (
              <button
                key={p}
                onClick={() => setAmount(((limit * p) / 100).toFixed(6))}
                className="flex-1 rounded-lg border border-[#c698e5]/15 py-1.5 text-[11px] font-medium text-[#efe0f7]/35 hover:border-[#c698e5]/35 hover:text-[#c698e5] transition-all cursor-pointer"
              >
                {p}%
              </button>
            ))}
          </div>

          {/* Info rows */}
          <div className="rounded-xl border border-[#c698e5]/10 divide-y divide-[#c698e5]/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                Supply APY
              </span>
              <span className="text-xs font-semibold text-emerald-400">
                {pool.supplyAPY.toFixed(2)}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                LTV
              </span>
              <span className="text-xs font-semibold text-[#efe0f7]/60">
                {poolData.ltv_percent}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                Collateral
              </span>
              <span className="text-xs font-semibold text-[#34d399]">
                Enabled
              </span>
            </div>
          </div>

          {/* Submit */}
          <button
            disabled={!canSubmit}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer flex items-center justify-center gap-2",
              canSubmit
                ? "bg-[#c698e5] text-[#17081f] hover:bg-[#d4aeee]"
                : "bg-[#c698e5]/12 text-[#efe0f7]/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            Deposit {pool.collateralSymbol}
          </button>
        </div>
      </div>
    </div>
  );
}

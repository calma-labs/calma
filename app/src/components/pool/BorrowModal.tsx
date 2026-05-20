import { useBorrow } from "@/hooks/program/useBorrow";
import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { sharesToAmount } from "@/lib/jblMath";
import { computeFeeBps } from "@/lib/poolDisplay";
import { cn } from "@/lib/utils";
import type { PoolData } from "@/types/lending";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { Info, Loader2, Lock, Wallet, X } from "lucide-react";
import { useMemo, useState } from "react";

interface BorrowModalProps {
  pool: Pool;
  poolData: PoolData;
  onClose: () => void;
}

export function BorrowModal({ pool, poolData, onClose }: BorrowModalProps) {
  const [amount, setAmount] = useState("");
  const [fixedRate, setFixedRate] = useState(false);
  const [fixedDuration, setFixedDuration] = useState<"1w" | "1m">("1w");
  const { wallet } = useWalletConnection();

  const { data: lendDecimals } = useMintDecimals(poolData.lendMint);
  const { data: collateralDecimals } = useMintDecimals(poolData.collateralMint);

  const walletPubKey = useMemo(
    () => (wallet ? new PublicKey(wallet.account.publicKey) : null),
    [wallet],
  );
  const { data: userPosition } = useUserPosition(
    poolData.publicKey,
    walletPubKey,
  );

  const borrowMutation = useBorrow();
  const isPending = borrowMutation.isPending;

  const displaySymbol = pool.lendSymbol;
  const displayIcon = pool.lendIcon;
  const userBorrowPower = useMemo(() => {
    if (!userPosition || collateralDecimals == null) return 0;
    const collateralUi =
      Number(userPosition.collateralDeposited) / 10 ** collateralDecimals;
    return collateralUi * (poolData.ltvPercent / 100);
  }, [userPosition, collateralDecimals, poolData.ltvPercent]);

  // Current debt (to subtract from borrow power)
  const currentDebtUi = useMemo(() => {
    if (!userPosition || !poolData || lendDecimals == null) return 0;
    if (poolData.totalDebtShares === 0n) return 0;
    const rawDebt =
      sharesToAmount(
        userPosition.debtShares,
        poolData.totalBorrowed,
        poolData.totalDebtShares,
      ) ?? 0n;
    return Number(rawDebt) / 10 ** lendDecimals;
  }, [userPosition, poolData, lendDecimals]);

  // Remaining borrow power, capped by pool available liquidity
  const limit = Math.max(
    0,
    Math.min(pool.availableLiquidity, userBorrowPower - currentDebtUi),
  );

  // Project borrow APY after this borrow based on post-borrow utilization.
  const DURATION_PREMIUM: Record<"1w" | "1m", number> = { "1w": 1.5, "1m": 1.5 * 1.08 };

  const projectedBorrowAPY = useMemo(() => {
    const decimals = lendDecimals ?? 6;
    const numAmount = parseFloat(amount);
    const borrowRaw = numAmount > 0 ? numAmount * 10 ** decimals : 0;
    const newTotalBorrowed = Number(poolData.totalBorrowed) + borrowRaw;
    const totalLend = Number(poolData.totalLendDeposited);
    const newUtilBps =
      totalLend > 0 ? Math.round((newTotalBorrowed / totalLend) * 10_000) : 0;
    const feeBps = computeFeeBps(poolData.feeConfig, newUtilBps);
    return feeBps / 100;
  }, [amount, lendDecimals, poolData]);

  const projectedFixedAPY = projectedBorrowAPY * DURATION_PREMIUM[fixedDuration];

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    const numAmount = parseFloat(amount);
    if (!numAmount || numAmount <= 0 || !wallet) return;

    const decimals = lendDecimals ?? 6;
    const rawAmount = new BN(Math.floor(numAmount * 10 ** decimals));

    await borrowMutation.mutateAsync({
      pool: poolData.publicKey,
      lendMint: poolData.lendMint,
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
          <p className="text-base font-semibold text-[#efe0f7]">Borrow</p>
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
                <span className="flex-shrink-0">Borrow power:</span>
                <span className="tabular-nums text-[#efe0f7]/55 truncate">
                  {limit.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 6,
                  })}{" "}
                  {displaySymbol}
                </span>
              </button>
            </div>

            <div className="flex items-center gap-2.5 rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/5 px-3.5 py-3 transition-colors focus-within:border-[#c698e5]/40">
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
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-[#efe0f7] placeholder-[#efe0f7]/20 outline-none tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-[11px] font-semibold text-[#efe0f7]/40 flex-shrink-0">
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
                className="flex-1 rounded-lg border border-[#c698e5]/15 py-1.5 text-[11px] font-medium text-[#efe0f7]/35 hover:border-[#c698e5]/35 hover:text-[#c698e5] transition-all cursor-pointer"
              >
                {p}%
              </button>
            ))}
          </div>

          {/* Fixed rate toggle */}
          <div className="flex flex-col gap-2">
            <label className="flex items-center justify-between rounded-xl border border-[#c698e5]/10 px-3.5 py-3 cursor-pointer hover:border-[#c698e5]/25 transition-colors">
              <div className="flex items-center gap-2.5">
                <Lock className="h-3.5 w-3.5 text-[#c698e5]/60" />
                <div>
                  <p className="text-xs font-medium text-[#efe0f7]/80">Fixed rate</p>
                  <p className="text-[11px] text-[#efe0f7]/35">
                    Lock in rate at {projectedFixedAPY.toFixed(2)}% APY
                  </p>
                </div>
              </div>
              <div
                className={cn(
                  "relative h-5 w-9 rounded-full transition-colors duration-200",
                  fixedRate ? "bg-[#c698e5]" : "bg-[#c698e5]/15",
                )}
              >
                <span
                  className={cn(
                    "absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform duration-200",
                    fixedRate ? "translate-x-4" : "translate-x-0.5",
                  )}
                />
                <input
                  type="checkbox"
                  checked={fixedRate}
                  onChange={(e) => setFixedRate(e.target.checked)}
                  className="sr-only"
                />
              </div>
            </label>

            {fixedRate && (
              <div className="flex gap-1.5">
                {(["1w", "1m"] as const).map((d) => (
                  <button
                    key={d}
                    onClick={() => setFixedDuration(d)}
                    className={cn(
                      "flex-1 rounded-lg border py-1.5 text-[11px] font-medium transition-all cursor-pointer",
                      fixedDuration === d
                        ? "border-[#c698e5]/50 bg-[#c698e5]/10 text-[#c698e5]"
                        : "border-[#c698e5]/15 text-[#efe0f7]/35 hover:border-[#c698e5]/35 hover:text-[#c698e5]",
                    )}
                  >
                    {d === "1w" ? "1 Week" : "1 Month"}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Info rows */}
          <div className="rounded-xl border border-[#c698e5]/10 divide-y divide-[#c698e5]/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                Borrow APY
              </span>
              <span className="text-xs font-semibold text-emerald-400">
                {fixedRate
                  ? projectedFixedAPY.toFixed(2)
                  : projectedBorrowAPY.toFixed(2)}
                %{fixedRate && (
                  <span className="ml-1 text-[10px] font-medium text-[#c698e5]/70">
                    fixed · {fixedDuration === "1w" ? "1w" : "1m"}
                  </span>
                )}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                Pool available
              </span>
              <span className="text-xs font-semibold tabular-nums text-[#efe0f7]/60">
                {pool.availableLiquidity.toLocaleString("en-US", {
                  maximumFractionDigits: 2,
                })}{" "}
                {displaySymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-[#efe0f7]/40">
                <Info className="h-3 w-3" />
                LTV ({poolData.ltvPercent}%)
              </span>
              <span className="text-xs font-semibold tabular-nums text-[#efe0f7]/60">
                {userBorrowPower.toLocaleString("en-US", {
                  maximumFractionDigits: 2,
                })}{" "}
                {displaySymbol}
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
                ? "bg-[#d45677] text-white hover:bg-[#e0647f]"
                : "bg-[#c698e5]/12 text-[#efe0f7]/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            Borrow {displaySymbol}
          </button>
        </div>
      </div>
    </div>
  );
}

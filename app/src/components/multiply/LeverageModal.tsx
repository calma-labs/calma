import { useOpenMultiply } from "@/hooks/program/useOpenMultiply";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { MAX_MULTIPLY } from "@/hooks/useMultiply";
import { useTokenBalance } from "@/hooks/useWalletBalances";
import { cn } from "@/lib/utils";
import type { PoolWithIrm } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import {
  Info,
  Loader2,
  Shield,
  TrendingUp,
  Wallet,
  X,
  Zap,
} from "lucide-react";
import { useMemo, useState } from "react";

interface LeverageModalProps {
  pool: Pool;
  poolData: PoolWithIrm;
  onClose: () => void;
}

/**
 * Modal for opening a leveraged (multiply) position.
 *
 * Flow: depositCollateral(amount) → flashBorrow(extra) → mockSwap →
 *       depositCollateral(extra) → borrow(extra+fee) → flashRepay(extra+fee)
 *
 * The user provides `amount` collateral tokens (their initial capital).
 * The flash loan and borrow cover the leverage — no lend tokens are spent.
 */
export function LeverageModal({ pool, poolData, onClose }: LeverageModalProps) {
  const [amount, setAmount] = useState("");
  const [leverage, setLeverage] = useState(1.5);
  const { wallet } = useWalletConnection();

  const walletPubKey = useMemo(() => {
    if (!wallet) return null;
    try {
      return new PublicKey(wallet.account.publicKey);
    } catch {
      return null;
    }
  }, [wallet]);

  const { data: collateralDecimals } = useMintDecimals(new PublicKey(poolData.collateral_mint));
  const decimals = collateralDecimals ?? 6;

  // User's collateral wallet balance (the token they deposit)
  const collateralBalance = useTokenBalance(new PublicKey(poolData.collateral_mint));
  const walletBalance = collateralBalance?.uiAmount ?? 0;

  const openMutation = useOpenMultiply();
  const isPending = openMutation.isPending;

  // Derived position metrics
  const amountNum = parseFloat(amount) || 0;
  const positionSize = amountNum * leverage;
  const borrowAmount = amountNum * (leverage - 1);
  const netAPY = poolData.leveraged_net_apy(leverage);
  const sliderPct = ((leverage - 1) / (MAX_MULTIPLY - 1)) * 100;

  const liquidationRisk =
    leverage < 5 ? "Low" : leverage < 15 ? "Moderate" : "High";
  const riskColor =
    liquidationRisk === "Low"
      ? "text-success"
      : liquidationRisk === "Moderate"
      ? "text-warning"
      : "text-destructive";

  const canSubmit =
    amountNum > 0 && amountNum <= walletBalance && !isPending && !!wallet;

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!canSubmit || !walletPubKey) return;

    const poolPubKey = new PublicKey(pool.address);
    const userCollateralAta = getAssociatedTokenAddressSync(
      new PublicKey(poolData.collateral_mint),
      walletPubKey,
    );
    const userLendAta = getAssociatedTokenAddressSync(
      new PublicKey(poolData.lend_mint),
      walletPubKey,
    );
    const amountRaw = new BN(Math.floor(amountNum * 10 ** decimals));

    await openMutation.mutateAsync({
      pool: poolPubKey,
      lendMint: new PublicKey(poolData.lend_mint),
      collateralMint: new PublicKey(poolData.collateral_mint),
      userCollateralAta,
      userLendAta,
      amountRaw,
      leverage,
    });

    onClose();
  }

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-md rounded-2xl border border-surface-accent/15 bg-modal-bg overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-surface-accent/10">
          <div className="flex items-center gap-3">
            <div className="relative">
              <img
                src={pool.lendIcon}
                alt={pool.lendSymbol}
                width={36}
                height={36}
                className="h-9 w-9 rounded-full ring-1 ring-surface-accent/20"
              />
              {/* style-exception: 18px icon size required for stacked icon overlap design */}
              <img
                src={pool.collateralIcon}
                alt={pool.collateralSymbol}
                width={18}
                height={18}
                className="absolute -bottom-0.5 -right-0.5 h-[18px] w-[18px] rounded-full ring-1 ring-modal-bg"
              />
            </div>
            <div>
              <p className="text-sm font-semibold text-surface-foreground">
                Multiply {pool.lendSymbol}
              </p>
              <p className="text-xs text-surface-foreground/35">
                Borrow {pool.lendSymbol} · up to {MAX_MULTIPLY}×
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-surface-foreground/30 hover:bg-surface-accent/10 hover:text-surface-foreground transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 pt-4 flex flex-col gap-5">
          {/* Amount input */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between px-1">
              <span className="text-xs text-surface-foreground/40">
                Initial collateral ({pool.collateralSymbol})
              </span>
              <button
                onClick={() => setAmount(walletBalance.toFixed(6))}
                className="flex items-center gap-1 text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer"
              >
                <Wallet className="h-3 w-3" />
                <span className="tabular-nums text-surface-foreground/50">
                  {walletBalance.toLocaleString("en-US", {
                    minimumFractionDigits: 2,
                    maximumFractionDigits: 6,
                  })}{" "}
                  {pool.collateralSymbol}
                </span>
              </button>
            </div>
            <div className="flex items-center gap-2.5 rounded-xl border border-surface-accent/15 bg-surface-accent/5 px-3.5 py-3 transition-colors focus-within:border-surface-accent/40">
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
                className="flex-1 min-w-0 bg-transparent text-sm font-semibold text-surface-foreground placeholder-surface-foreground/20 tabular-nums [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
              />
              <span className="text-xs font-semibold text-surface-foreground/40 flex-shrink-0">
                {pool.collateralSymbol}
              </span>
            </div>
          </div>

          {/* Leverage slider */}
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between px-1">
              <span className="text-xs text-surface-foreground/40">Multiplier</span>
              <span className="text-sm font-bold text-surface-accent tabular-nums">
                {leverage.toFixed(1)}×
              </span>
            </div>
            {/* style-exception: slider gradient must be set via style to reflect current value dynamically */}
            <input
              type="range"
              min={1}
              max={MAX_MULTIPLY}
              step={0.1}
              value={leverage}
              onChange={(e) => setLeverage(parseFloat(e.target.value))}
              className="w-full h-1.5 appearance-none rounded-full cursor-pointer
                [&::-webkit-slider-runnable-track]:rounded-full
                [&::-webkit-slider-runnable-track]:h-1.5
                [&::-webkit-slider-thumb]:appearance-none
                [&::-webkit-slider-thumb]:h-4
                [&::-webkit-slider-thumb]:w-4
                [&::-webkit-slider-thumb]:rounded-full
                [&::-webkit-slider-thumb]:bg-surface-accent
                [&::-webkit-slider-thumb]:shadow-[0_0_10px_rgba(198,152,229,0.5)]
                [&::-webkit-slider-thumb]:border-2
                [&::-webkit-slider-thumb]:border-modal-bg
                [&::-webkit-slider-thumb]:-mt-[3px]"
              style={{
                background: `linear-gradient(to right, #c698e5 0%, #c698e5 ${sliderPct}%, rgba(198,152,229,0.15) ${sliderPct}%, rgba(198,152,229,0.15) 100%)`,
              }}
            />
            <div className="flex justify-between px-0.5">
              {[1, 5, 10, 15, 20, 25, 30].map((v) => (
                <button
                  key={v}
                  onClick={() => setLeverage(v)}
                  className={cn(
                    "text-xs font-medium transition-colors cursor-pointer",
                    Math.round(leverage) === v
                      ? "text-surface-accent"
                      : "text-surface-foreground/25 hover:text-surface-foreground/60",
                  )}
                >
                  {v}×
                </button>
              ))}
            </div>
          </div>

          {/* Position summary */}
          <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <TrendingUp className="h-3 w-3" />
                Position size
              </span>
              <span className="text-xs font-semibold text-surface-foreground/80 tabular-nums">
                {positionSize > 0
                  ? `${positionSize.toLocaleString("en-US", {
                      maximumFractionDigits: 4,
                    })} ${pool.collateralSymbol}`
                  : "—"}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Borrow ({pool.lendSymbol})
              </span>
              <span className="text-xs font-semibold text-surface-foreground/80 tabular-nums">
                {borrowAmount > 0
                  ? `${borrowAmount.toLocaleString("en-US", {
                      maximumFractionDigits: 4,
                    })} ${pool.lendSymbol}`
                  : "—"}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Zap className="h-3 w-3" />
                Net APY
              </span>
              <span className="text-xs font-bold text-success tabular-nums">
                {netAPY.toFixed(2)}%
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Shield className="h-3 w-3" />
                Liquidation risk
              </span>
              <span className={cn("text-xs font-semibold", riskColor)}>
                {liquidationRisk}
              </span>
            </div>
          </div>

          <button
            disabled={!canSubmit}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] flex items-center justify-center gap-2",
              canSubmit
                ? "bg-surface-accent text-surface hover:bg-surface-accent/80 cursor-pointer shadow-[0_0_24px_rgba(198,152,229,0.30)]"
                : "bg-surface-accent/10 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Opening…
              </>
            ) : (
              `Open ${leverage.toFixed(1)}× Position`
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

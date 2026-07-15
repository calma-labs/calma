import { useBorrow } from "@/hooks/program/useBorrow";
import {
  useFeedFreshness,
  type FeedFreshness,
  type OraclePriceSide,
} from "@/hooks/program/useFeedFreshness";
import { useNow } from "@/hooks/useNow";
import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { cn } from "@/lib/utils";
import type { PoolWithIrm } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { ChevronDown, Info, Loader2, Lock, Wallet, X } from "lucide-react";
import { useMemo, useState } from "react";
import { BN } from "@anchor-lang/core";

interface BorrowModalProps {
  pool: Pool;
  poolData: PoolWithIrm;
  onClose: () => void;
}

function OracleTable({ freshness }: { freshness: FeedFreshness }) {
  const anyStale =
    freshness.onChain.collateral.stale ||
    freshness.onChain.lend.stale ||
    (freshness.hermes?.collateral.stale ?? false) ||
    (freshness.hermes?.lend.stale ?? false);
  // Auto-open when anything is stale; user can still toggle. Collapse default
  // hides all four cells but shows the row label + a compact status pip so
  // users know at a glance that the oracle data is being watched.
  const [open, setOpen] = useState(anyStale);

  return (
    <div className="rounded-xl border border-surface-accent/10 overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center justify-between px-3.5 py-2.5 text-xs text-surface-foreground/50 hover:bg-surface-accent/5 transition-colors cursor-pointer"
      >
        <span className="flex items-center gap-1.5">
          <Info className="h-3 w-3" />
          Oracle prices
        </span>
        <span className="flex items-center gap-2">
          <span
            className={cn(
              "h-1.5 w-1.5 rounded-full",
              anyStale ? "bg-destructive" : "bg-success",
            )}
          />
          <ChevronDown
            className={cn(
              "h-3.5 w-3.5 transition-transform",
              open && "rotate-180",
            )}
          />
        </span>
      </button>
      {open && (
        <div className="grid grid-cols-[auto_1fr_1fr] text-xs border-t border-surface-accent/8">
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/40" />
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/50 font-medium">
            Collateral
          </div>
          <div className="bg-surface-accent/5 px-3 py-2 text-surface-foreground/50 font-medium">
            Lend
          </div>

          <div className="border-t border-surface-accent/8 px-3 py-2 text-surface-foreground/40 flex flex-col justify-center">
            <span>On-chain</span>
            <span className="text-surface-foreground/30">
              max {freshness.onChain.maxAgeSecs}s
            </span>
          </div>
          <PriceCell side={freshness.onChain.collateral} />
          <PriceCell side={freshness.onChain.lend} />

          <div className="border-t border-surface-accent/8 px-3 py-2 text-surface-foreground/40 flex flex-col justify-center">
            <span>Hermes</span>
            <span className="text-surface-foreground/30">
              max {freshness.hermes?.maxAgeSecs ?? "—"}s
            </span>
          </div>
          <PriceCell side={freshness.hermes?.collateral} />
          <PriceCell side={freshness.hermes?.lend} />
        </div>
      )}
    </div>
  );
}

function PriceCell({ side }: { side: OraclePriceSide | undefined }) {
  const now = useNow();
  const stale = side?.stale ?? false;
  const ageSecs = side?.publishTs != null ? now - side.publishTs : null;
  return (
    <div
      className={cn(
        "border-t border-surface-accent/8 px-3 py-2 tabular-nums",
        stale ? "text-destructive" : "text-surface-foreground/70",
      )}
    >
      <div className="font-semibold">
        {side?.price != null
          ? `$${side.price.toLocaleString("en-US", {
              minimumFractionDigits: 2,
              maximumFractionDigits: 6,
            })}`
          : "—"}
      </div>
      <div className="text-surface-foreground/40">
        {ageSecs != null ? `${ageSecs}s` : "—"}
      </div>
    </div>
  );
}

export function BorrowModal({ pool, poolData, onClose }: BorrowModalProps) {
  const [amount, setAmount] = useState("");
  const [fixedRate, setFixedRate] = useState(false);
  const [fixedDuration, setFixedDuration] = useState<"1w" | "1m">("1w");
  const { wallet } = useWalletConnection();

  const { data: lendDecimals } = useMintDecimals(new PublicKey(poolData.lend_mint));

  const walletPubKey = useMemo(
    () => (wallet ? new PublicKey(wallet.account.publicKey) : null),
    [wallet],
  );
  const { data: userPosition } = useUserPosition(
    new PublicKey(pool.address),
    walletPubKey,
  );
  const borrowMutation = useBorrow();
  const isPending = borrowMutation.isPending;
  const { data: freshness } = useFeedFreshness(new PublicKey(pool.address));
  const feedStale = freshness?.willFail ?? false;

  const displaySymbol = pool.lendSymbol;
  const displayIcon = pool.lendIcon;
  // On-chain: max_borrowable = collateral_raw * ltv / 100 (raw lend units)
  const userBorrowPower = useMemo(() => {
    if (!userPosition || lendDecimals == null) return 0;
    return Number(poolData.max_borrowable(userPosition)) / 10 ** lendDecimals;
  }, [userPosition, lendDecimals, poolData]);

  // Current debt (to subtract from borrow power)
  const currentDebtUi = useMemo(() => {
    if (!userPosition || lendDecimals == null) return 0;
    return Number(poolData.debt_amount(userPosition) ?? 0n) / 10 ** lendDecimals;
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
    const borrowRaw = numAmount > 0 ? BigInt(Math.round(numAmount * 10 ** decimals)) : 0n;
    return poolData.projected_borrow_apy_bps(borrowRaw) / 100;
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
      pool: new PublicKey(pool.address),
      lendMint: new PublicKey(poolData.lend_mint),
      feedState: new PublicKey(poolData.pool().feed_state),
      amount: rawAmount,
    });

    onClose();
  }

  const canSubmit =
    !!amount && parseFloat(amount) > 0 && !isPending && !!wallet && !feedStale;

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
          <p className="text-base font-semibold text-surface-foreground">Borrow</p>
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
                onClick={() => setAmount(String(limit))}
                className="flex items-center gap-1 text-xs text-surface-foreground/35 hover:text-surface-accent transition-colors cursor-pointer min-w-0"
              >
                <Wallet className="h-3 w-3 flex-shrink-0" />
                <span className="flex-shrink-0">Borrow power:</span>
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

          {/* Fixed rate toggle */}
          <div className="flex flex-col gap-2">
            <label className="flex items-center justify-between rounded-xl border border-surface-accent/10 px-3.5 py-3 cursor-pointer hover:border-surface-accent/25 transition-colors">
              <div className="flex items-center gap-2.5">
                <Lock className="h-3.5 w-3.5 text-surface-accent/60" />
                <div>
                  <p className="text-xs font-medium text-surface-foreground/80">Fixed rate</p>
                  <p className="text-xs text-surface-foreground/35">
                    Lock in rate at {projectedFixedAPY.toFixed(2)}% APY
                  </p>
                </div>
              </div>
              <div
                className={cn(
                  "relative h-5 w-9 rounded-full transition-colors duration-200",
                  fixedRate ? "bg-surface-accent" : "bg-surface-accent/15",
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
                      "flex-1 rounded-lg border py-1.5 text-xs font-medium transition-all cursor-pointer",
                      fixedDuration === d
                        ? "border-surface-accent/50 bg-surface-accent/10 text-surface-accent"
                        : "border-surface-accent/15 text-surface-foreground/35 hover:border-surface-accent/35 hover:text-surface-accent",
                    )}
                  >
                    {d === "1w" ? "1 Week" : "1 Month"}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Info rows */}
          <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Borrow APY
              </span>
              <span className="text-xs font-semibold text-success">
                {fixedRate
                  ? projectedFixedAPY.toFixed(2)
                  : projectedBorrowAPY.toFixed(2)}
                %{fixedRate && (
                  <span className="ml-1 text-xs font-medium text-surface-accent/70">
                    fixed · {fixedDuration === "1w" ? "1w" : "1m"}
                  </span>
                )}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                Pool available
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/60">
                {pool.availableLiquidity.toLocaleString("en-US", {
                  maximumFractionDigits: 2,
                })}{" "}
                {displaySymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="flex items-center gap-1.5 text-xs text-surface-foreground/40">
                <Info className="h-3 w-3" />
                LTV ({poolData.ltv_percent}%)
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/60">
                {userBorrowPower.toLocaleString("en-US", {
                  maximumFractionDigits: 2,
                })}{" "}
                {displaySymbol}
              </span>
            </div>
          </div>

          {freshness && <OracleTable freshness={freshness} />}

          {/* Submit */}
          <button
            disabled={!canSubmit}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] cursor-pointer flex items-center justify-center gap-2",
              canSubmit
                ? "bg-destructive text-white hover:bg-destructive/80"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending && <Loader2 className="h-4 w-4 animate-spin" />}
            {feedStale ? "Oracle stale" : `Borrow ${displaySymbol}`}
          </button>
        </div>
      </div>
    </div>
  );
}

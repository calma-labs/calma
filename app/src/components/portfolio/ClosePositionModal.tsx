import { useCloseMultiply } from "@/hooks/program/useCloseMultiply";
import { useFeedFreshness } from "@/hooks/program/useFeedFreshness";
import { OracleTable } from "@/components/common/OracleTable";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { cn } from "@/lib/utils";
import type { PoolWithIrm, UserPositionAccount } from "@jbl/wasm-lib";
import { flash_fee } from "@jbl/wasm-lib";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { AlertTriangle, Loader2, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { ManageMultiplyPosition } from "./ManagePositionModal";

// Re-export for backwards-compat with callers that import CloseMultiplyPosition
export type CloseMultiplyPosition = ManageMultiplyPosition;

interface ClosePositionModalProps {
  pool: Pool;
  poolData: PoolWithIrm;
  userPosition: UserPositionAccount;
  /** Pre-computed display data (leverage, netAPY, etc.). */
  position: ManageMultiplyPosition;
  onClose: () => void;
}

/**
 * Confirms and executes the flash-loan unwind to close a multiply position.
 *
 * Transaction: flashBorrow(debt) → repay(debt) → withdraw(collateral) →
 *              mockSwap(collateral, col→lend) → flashRepay(debt + fee)
 *
 * Net result: user receives (collateral − debt − fee) lend tokens.
 */
export function ClosePositionModal({
  pool,
  poolData,
  userPosition,
  position,
  onClose,
}: ClosePositionModalProps) {
  const [confirmed, setConfirmed] = useState(false);
  const { wallet } = useWalletConnection();

  const { data: lendDecimals } = useMintDecimals(new PublicKey(poolData.lend_mint));
  const { data: collateralDecimals } = useMintDecimals(new PublicKey(poolData.collateral_mint));

  const closeMutation = useCloseMultiply();
  const isPending = closeMutation.isPending;

  const poolPubKey = useMemo(() => new PublicKey(pool.address), [pool.address]);
  const { data: freshness } = useFeedFreshness(poolPubKey);
  const feedStale = freshness?.willFail ?? false;

  // Raw debt derived from debt shares via WASM (interest accrued to now)
  const debtRaw = useMemo(
    () => poolData.debt_amount(userPosition) ?? 0n,
    [userPosition, poolData],
  );

  const collateralRaw = userPosition.collateral_deposited;

  // Numeric amounts needed for flash-fee and estimated-return math
  const debtUi = Number(debtRaw) / 10 ** (lendDecimals ?? 6);
  const collateralUi = Number(collateralRaw) / 10 ** (collateralDecimals ?? 6);
  const flashFee =
    Number(flash_fee(debtRaw) ?? 0n) / 10 ** (lendDecimals ?? 6);
  const estimatedReturn = Math.max(0, collateralUi - debtUi - flashFee);

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!confirmed || !wallet || isPending || feedStale) return;

    const walletPubKey = new PublicKey(wallet.account.publicKey);
    const userCollateralAta = getAssociatedTokenAddressSync(
      new PublicKey(poolData.collateral_mint),
      walletPubKey,
    );
    const userLendAta = getAssociatedTokenAddressSync(
      new PublicKey(poolData.lend_mint),
      walletPubKey,
    );

    await closeMutation.mutateAsync({
      pool: poolPubKey,
      lendMint: new PublicKey(poolData.lend_mint),
      collateralMint: new PublicKey(poolData.collateral_mint),
      feedState: new PublicKey(poolData.pool().feed_state),
      userCollateralAta,
      userLendAta,
      debtRaw: new BN(debtRaw.toString()),
      collateralRaw: new BN(collateralRaw.toString()),
    });

    onClose();
  }

  return (
    <div
      onClick={handleBackdrop}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4"
    >
      <div
        className="w-full max-w-sm rounded-2xl border border-surface-accent/15 bg-modal-bg overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-surface-accent/10">
          <div className="flex items-center gap-3">
            <img
              src={position.icon}
              alt={position.asset}
              className="h-9 w-9 rounded-full ring-1 ring-surface-accent/20"
            />
            <div>
              <p className="text-sm font-semibold text-surface-foreground">
                Close Position
              </p>
              <p className="text-xs text-surface-foreground/35">
                {position.asset} ·{" "}
                <span className="text-surface-accent">
                  {position.multiplier.toFixed(2)}×
                </span>{" "}
                · debt {position.debtAsset}
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

        <div className="px-5 pb-5 pt-4 flex flex-col gap-4">
          {/* Position summary */}
          <div className="rounded-xl border border-surface-accent/10 divide-y divide-surface-accent/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-surface-foreground/40">
                Collateral to withdraw
              </span>
              <span className="text-xs font-semibold tabular-nums text-surface-foreground/70">
                {userPosition.format_collateral(collateralDecimals ?? 6)}{" "}
                {pool.collateralSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-surface-foreground/40">Debt to repay</span>
              <span className="text-xs font-semibold tabular-nums text-destructive">
                {poolData.format_debt(userPosition, lendDecimals ?? 6)}{" "}
                {pool.lendSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-surface-foreground/40">
                Flash fee (0.09%)
              </span>
              <span className="text-xs tabular-nums text-surface-foreground/50">
                ~
                {flashFee.toLocaleString("en-US", { maximumFractionDigits: 6 })}{" "}
                {pool.lendSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-surface-foreground/40">Est. return</span>
              <span
                className={cn(
                  "text-xs font-bold tabular-nums",
                  estimatedReturn > 0 ? "text-success" : "text-destructive",
                )}
              >
                ~
                {estimatedReturn.toLocaleString("en-US", {
                  maximumFractionDigits: 6,
                })}{" "}
                {pool.lendSymbol}
              </span>
            </div>
          </div>

          {/* Warning */}
          <div className="rounded-xl border border-warning/15 bg-warning/5 px-3.5 py-2.5 flex items-start gap-2">
            <AlertTriangle className="h-3.5 w-3.5 text-warning mt-0.5 flex-shrink-0" />
            <p className="text-xs text-warning/80 leading-relaxed">
              Closing repays all debt and returns collateral as{" "}
              {pool.lendSymbol} via a flash-loan unwind. If pool utilization is
              too high the withdrawal may be queued and the transaction will
              revert safely.
            </p>
          </div>

          {/* Confirmation checkbox */}
          <label className="flex items-center gap-2.5 cursor-pointer group">
            <div
              onClick={() => setConfirmed((c) => !c)}
              className={cn(
                "h-4 w-4 rounded flex-shrink-0 border transition-all cursor-pointer flex items-center justify-center",
                confirmed
                  ? "bg-surface-accent border-surface-accent"
                  : "border-surface-accent/30 bg-transparent hover:border-surface-accent/60",
              )}
            >
              {confirmed && (
                <svg
                  className="h-2.5 w-2.5 text-surface"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={3}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M5 13l4 4L19 7"
                  />
                </svg>
              )}
            </div>
            <span className="text-xs text-surface-foreground/45 group-hover:text-surface-foreground/65 transition-colors">
              I understand this action is irreversible
            </span>
          </label>

          {freshness && <OracleTable freshness={freshness} />}

          <button
            disabled={!confirmed || isPending || feedStale}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] flex items-center justify-center gap-2",
              confirmed && !isPending && !feedStale
                ? "bg-destructive text-white hover:bg-destructive/80 cursor-pointer"
                : "bg-surface-accent/12 text-surface-foreground/20 cursor-not-allowed",
            )}
          >
            {isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Closing…
              </>
            ) : feedStale ? (
              "Oracle stale"
            ) : (
              "Close Position"
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

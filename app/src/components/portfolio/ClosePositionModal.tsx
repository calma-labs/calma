import { useCloseMultiply } from "@/hooks/program/useCloseMultiply";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { cn } from "@/lib/utils";
import type { PoolData } from "@/types/lending";
import type { UserPositionAccount } from "@jbl/wasm-lib";
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
  poolData: PoolData;
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

  const { data: lendDecimals } = useMintDecimals(poolData.lendMint);
  const { data: collateralDecimals } = useMintDecimals(poolData.collateralMint);

  const closeMutation = useCloseMultiply();
  const isPending = closeMutation.isPending;

  // Raw debt derived from debt shares via WASM
  const debtRaw = useMemo(
    () => userPosition.debt_amount(poolData.totalBorrowed, poolData.totalDebtShares),
    [userPosition, poolData.totalBorrowed, poolData.totalDebtShares],
  );

  const collateralRaw = userPosition.collateral_deposited;

  // Numeric amounts needed for flash-fee and estimated-return math
  const debtUi = Number(debtRaw) / 10 ** (lendDecimals ?? 6);
  const collateralUi = Number(collateralRaw) / 10 ** (collateralDecimals ?? 6);
  const flashFee =
    Math.floor((Number(debtRaw) * 9) / 10_000) / 10 ** (lendDecimals ?? 6);
  const estimatedReturn = Math.max(0, collateralUi - debtUi - flashFee);

  function handleBackdrop(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }

  async function handleSubmit() {
    if (!confirmed || !wallet || isPending) return;

    const walletPubKey = new PublicKey(wallet.account.publicKey);
    const poolPubKey = new PublicKey(pool.address);
    const userCollateralAta = getAssociatedTokenAddressSync(
      poolData.collateralMint,
      walletPubKey,
    );
    const userLendAta = getAssociatedTokenAddressSync(
      poolData.lendMint,
      walletPubKey,
    );

    await closeMutation.mutateAsync({
      pool: poolPubKey,
      lendMint: poolData.lendMint,
      collateralMint: poolData.collateralMint,
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
        className="w-full max-w-sm rounded-2xl border border-[#c698e5]/15 bg-[#1a0d24] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-5 pb-4 border-b border-[#c698e5]/10">
          <div className="flex items-center gap-3">
            <img
              src={position.icon}
              alt={position.asset}
              className="h-9 w-9 rounded-full ring-1 ring-[#c698e5]/20"
            />
            <div>
              <p className="text-sm font-semibold text-[#efe0f7]">
                Close Position
              </p>
              <p className="text-[11px] text-[#efe0f7]/35">
                {position.asset} ·{" "}
                <span className="text-[#c698e5]">
                  {position.multiplier.toFixed(2)}×
                </span>{" "}
                · debt {position.debtAsset}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-[#efe0f7]/30 hover:bg-[#c698e5]/10 hover:text-[#efe0f7] transition-colors cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pb-5 pt-4 flex flex-col gap-4">
          {/* Position summary */}
          <div className="rounded-xl border border-[#c698e5]/10 divide-y divide-[#c698e5]/8">
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-[#efe0f7]/40">
                Collateral to withdraw
              </span>
              <span className="text-xs font-semibold tabular-nums text-[#efe0f7]/70">
                {userPosition.format_collateral(collateralDecimals ?? 6)}{" "}
                {pool.collateralSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-[#efe0f7]/40">Debt to repay</span>
              <span className="text-xs font-semibold tabular-nums text-[#d45677]">
                {userPosition.format_debt(poolData.totalBorrowed, poolData.totalDebtShares, lendDecimals ?? 6)}{" "}
                {pool.lendSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-[#efe0f7]/40">
                Flash fee (0.09%)
              </span>
              <span className="text-xs tabular-nums text-[#efe0f7]/50">
                ~
                {flashFee.toLocaleString("en-US", { maximumFractionDigits: 6 })}{" "}
                {pool.lendSymbol}
              </span>
            </div>
            <div className="flex items-center justify-between px-3.5 py-2.5">
              <span className="text-xs text-[#efe0f7]/40">Est. return</span>
              <span
                className={cn(
                  "text-xs font-bold tabular-nums",
                  estimatedReturn > 0 ? "text-[#34d399]" : "text-[#d45677]",
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
          <div className="rounded-xl border border-[#f0a854]/15 bg-[#f0a854]/5 px-3.5 py-2.5 flex items-start gap-2">
            <AlertTriangle className="h-3.5 w-3.5 text-[#f0a854] mt-0.5 flex-shrink-0" />
            <p className="text-[11px] text-[#f0a854]/80 leading-relaxed">
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
                  ? "bg-[#c698e5] border-[#c698e5]"
                  : "border-[#c698e5]/30 bg-transparent hover:border-[#c698e5]/60",
              )}
            >
              {confirmed && (
                <svg
                  className="h-2.5 w-2.5 text-[#17081f]"
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
            <span className="text-[11px] text-[#efe0f7]/45 group-hover:text-[#efe0f7]/65 transition-colors">
              I understand this action is irreversible
            </span>
          </label>

          <button
            disabled={!confirmed || isPending}
            onClick={handleSubmit}
            className={cn(
              "w-full rounded-xl py-3 text-sm font-semibold transition-all duration-200 active:scale-[0.98] flex items-center justify-center gap-2",
              confirmed && !isPending
                ? "bg-[#d45677] text-white hover:bg-[#e0647f] cursor-pointer"
                : "bg-[#c698e5]/12 text-[#efe0f7]/20 cursor-not-allowed",
            )}
          >
            {isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Closing…
              </>
            ) : (
              "Close Position"
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

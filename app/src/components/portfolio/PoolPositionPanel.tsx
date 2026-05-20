import { type PoolData } from "@/hooks/program/useLendingAccount";
import { useRepay } from "@/hooks/program/useRepay";
import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { useTokenBalance } from "@/hooks/useWalletBalances";
import { sharesToAmount } from "@/lib/jblMath";
import { cn } from "@/lib/utils";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { useMemo, useState } from "react";
import { HealthBadge } from "../common/Badge";
import { PositionActionButton } from "../common/PositionActionButton";
import { LeaveModal } from "../pool/LeaveModal";
// import { PutLpModal } from "./PutLpModal";
import { RepayModal, type RepayPosition } from "./RepayModal";
// import { TakeLpModal } from "./TakeLpModal";
import { type WithdrawPosition } from "./WithdrawModal";

// ─── Sub-components ──────────────────────────────────────────────────────────

function SectionLabel({ label }: { label: string }) {
  return (
    <div className="px-4 border-b border-[#c698e5]/8 bg-[#c698e5]/[0.015]">
      <span className="text-[10px] font-semibold uppercase tracking-wider text-[#efe0f7]/30">
        {label}
      </span>
    </div>
  );
}

function LendRow({
  pos,
  // onClaimLp,
  // onPutLp,
  onRedeemLp,
}: {
  pos: WithdrawPosition;
  // onClaimLp?: () => void;
  // onPutLp?: () => void;
  onRedeemLp?: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-10 gap-y-2 px-4 py-3.5 border-b border-[#c698e5]/8 last:border-none">
      {/* Asset */}
      <div className="flex items-center gap-2 min-w-[90px]">
        <img src={pos.icon} alt={pos.asset} className="h-6 w-6 rounded-full" />
        <p className="text-sm font-semibold text-[#efe0f7]">{pos.asset}</p>
      </div>

      {/* Supplied */}
      <div className="flex flex-col min-w-[80px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Supplied</span>
        <span className="text-sm font-semibold tabular-nums text-[#efe0f7]">
          {pos.supplied.toLocaleString("en-US", { maximumFractionDigits: 4 })}
        </span>
      </div>

      {/* APY */}
      <div className="flex flex-col min-w-[60px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">APY</span>
        <span className="text-sm font-semibold tabular-nums text-[#34d399]">
          {pos.apy.toFixed(2)}%
        </span>
      </div>

      <div className="flex flex-col min-w-[70px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Earned</span>
        <span className="text-sm tabular-nums text-[#34d399]">
          {/* +${pos.earned < 1 ? pos.earned.toFixed(3) : pos.earned.toFixed(2)} */}
          {/* Placeholder until we fetch real earned amounts */}
          +$11.23
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-[10px] ml-1 text-[#efe0f7]/35 mb-0.5">
          Health
        </span>
        {/* <HealthBadge value={pos.health} /> */}
        {/* Placeholder until we compute real health values */}
        <HealthBadge value={90.32} />
      </div>

      {/* Collateral */}
      <div className="flex flex-col min-w-[50px]">
        <span className="text-[10px] ml-1 text-[#efe0f7]/35 mb-0.5">
          Collateral
        </span>
        <span
          className={cn(
            "text-[10px] px-2 py-0.5 rounded-full font-medium w-fit",
            pos.collateralEnabled
              ? "bg-[#34d399]/10 text-[#34d399]"
              : "bg-[#efe0f7]/8 text-[#efe0f7]/35",
          )}
        >
          {pos.collateralEnabled ? "On" : "Off"}
        </span>
      </div>

      <div className="flex items-center gap-2 ml-auto">
        {/* {onClaimLp && (
          <PositionActionButton label="Take LP" onClick={onClaimLp} />
        )}
        {onPutLp && <PositionActionButton label="Put LP" onClick={onPutLp} />} */}
        {onRedeemLp && (
          <PositionActionButton label="Withdraw" onClick={onRedeemLp} />
        )}
      </div>
    </div>
  );
}

function BorrowRow({
  pos,
  onRepay,
}: {
  pos: RepayPosition;
  onRepay: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-10 gap-y-2 px-4 py-3.5 border-b border-[#c698e5]/8 last:border-none">
      {/* Borrowed asset (primary) */}
      <div className="flex items-center gap-2 min-w-[90px]">
        <img
          src={pos.borrowedIcon}
          alt={pos.borrowedAsset}
          className="h-6 w-6 rounded-full"
        />
        <p className="text-sm font-semibold text-[#efe0f7]">
          {pos.borrowedAsset}
        </p>
      </div>

      {/* Lend */}
      {/* <div className="flex items-center gap-2 min-w-[90px]">
        <div className="flex flex-col">
          <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Lended</span>

          <div className="flex items-center gap-1.5">
            <img
              src={pos.borrowedIcon}
              alt={pos.borrowedAsset}
              className="h-4 w-4 rounded-full"
            />
            <span className="text-xs text-[#efe0f7]/50">
              {pos.borrowedAsset}
            </span>
          </div>
        </div>
      </div> */}

      {/* Debt */}
      <div className="flex flex-col min-w-[80px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Debt</span>
        <span className="text-sm font-semibold tabular-nums text-[#efe0f7]">
          {pos.debtAmount.toLocaleString("en-US", { maximumFractionDigits: 4 })}
        </span>
      </div>

      {/* Borrow APY */}
      <div className="flex flex-col min-w-[60px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Borrow APY</span>
        <span className="text-sm font-semibold tabular-nums text-[#d45677]">
          {pos.borrowAPY.toFixed(2)}%
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">LTV</span>
        <span className="text-sm tabular-nums text-[#efe0f7]/70">
          {/* {pos.ltv.toFixed(1)}% */}
          {/* Placeholder until we compute real LTV values */}
          45.2%
        </span>
      </div>

      <div className="flex flex-col min-w-[70px]">
        <span className="text-[10px] text-[#efe0f7]/35 mb-0.5">Liq. Price</span>
        <span className="text-sm tabular-nums text-[#efe0f7]/70">
          {/* ${pos.liqPrice.toFixed(1)} */}
          {/* Placeholder until we compute real liquidation price */}
          $123.45
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-[10px] ml-1 text-[#efe0f7]/35 mb-0.5">HF</span>
        {/* <HFBadge value={pos.healthFactor} /> */}
        {/* Placeholder until we compute real health factor */}
        <HealthBadge value={90.32} />
      </div>

      <div className="flex items-center gap-2 ml-auto">
        <PositionActionButton label="Repay" onClick={onRepay} />
      </div>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

type ModalState =
  | { type: "repay"; pos: RepayPosition }
  // | { type: "takeLp" }
  // | { type: "putLp" }
  | { type: "leaveLp" }
  | null;

interface PoolPositionPanelProps {
  pool: Pool;
  poolData: PoolData;
  connected: boolean;
}

export function PoolPositionPanel({
  pool,
  poolData,
  connected,
}: PoolPositionPanelProps) {
  const [modal, setModal] = useState<ModalState>(null);
  const { wallet } = useWalletConnection();

  const poolPubKey = useMemo(() => {
    try {
      return new PublicKey(pool.address);
    } catch {
      return null;
    }
  }, [pool.address]);

  const walletPubKey = useMemo(() => {
    if (!wallet) return null;
    try {
      return new PublicKey(wallet.account.publicKey);
    } catch {
      return null;
    }
  }, [wallet]);

  const { data: userPosition, isLoading: positionLoading } = useUserPosition(
    poolPubKey,
    walletPubKey,
  );
  const { data: lendDecimals } = useMintDecimals(poolData.lendMint ?? null);
  const lpWalletBalance = useTokenBalance(poolData.lpMint ?? null);
  const lendWalletBalance = useTokenBalance(poolData.lendMint ?? null);

  const repayMutation = useRepay();

  // Compute on-chain amounts as human-readable numbers
  const debtUiAmount = useMemo(() => {
    if (!userPosition || !poolData || lendDecimals == null) return null;
    if (poolData.totalDebtShares === 0n) return 0;
    const rawDebt =
      sharesToAmount(
        userPosition.debtShares,
        poolData.totalBorrowed,
        poolData.totalDebtShares,
      ) ?? 0n;
    return Number(rawDebt) / 10 ** lendDecimals;
  }, [userPosition, poolData, lendDecimals]);

  // LP wallet balance drives the Lend section (LP tokens are in user's wallet ATA)
  const hasLp = (lpWalletBalance?.uiAmount ?? 0) > 0;
  const hasDebt = userPosition != null && userPosition.debtShares > 0n;

  const lendPos: WithdrawPosition | null = hasLp
    ? {
      asset: pool.lendSymbol,
      icon: pool.lendIcon,
      supplied: lpWalletBalance?.uiAmount ?? 0,
      apy: pool.supplyAPY,
      collateralEnabled: true,
    }
    : null;

  const borrowPos: RepayPosition | null = hasDebt
    ? {
      collateralAsset: pool.collateralSymbol,
      collateralIcon: pool.collateralIcon,
      borrowedAsset: pool.lendSymbol,
      borrowedIcon: pool.lendIcon,
      debtAmount: debtUiAmount ?? 0,
      rawDebtAmount: userPosition && poolData && lendDecimals != null && poolData.totalDebtShares > 0n
        ? (sharesToAmount(userPosition.debtShares, poolData.totalBorrowed, poolData.totalDebtShares) ?? 0n).toString()
        : undefined,
      borrowAPY: pool.borrowAPY,
      walletBalance: lendWalletBalance?.uiAmount ?? undefined,
    }
    : null;

  async function handleRepay(amount: number, rawAmountStr?: string) {
    if (!poolData || !poolPubKey) return;
    // Use raw amount if provided (for max repayment), otherwise calculate from UI amount
    const rawAmount = rawAmountStr ? new BN(rawAmountStr) : new BN(Math.floor(amount * 10 ** (lendDecimals ?? 6)));
    await repayMutation.mutateAsync({
      pool: poolPubKey,
      lendMint: poolData.lendMint,
      amount: rawAmount,
    });
  }

  if (!connected) return null;
  if (positionLoading) {
    return (
      <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.02] px-4 py-6 flex items-center gap-2">
        <div className="h-1.5 w-1.5 rounded-full bg-[#c698e5] animate-pulse" />
        <span className="text-[11px] text-[#efe0f7]/30">
          Loading positions…
        </span>
      </div>
    );
  }
  if (!lendPos && !borrowPos) return null;

  return (
    <>
      <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.02] overflow-hidden">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-[#c698e5]/10">
          <div className="h-1.5 w-1.5 rounded-full bg-[#c698e5]" />
          <span className="text-[11px] font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            My Positions
          </span>
        </div>

        {lendPos && (
          <>
            <SectionLabel label="Lend" />
            <LendRow
              pos={lendPos}
              onRedeemLp={
                true ? () => setModal({ type: "leaveLp" }) : undefined
              }
            />
          </>
        )}

        {borrowPos && (
          <>
            <SectionLabel label="Borrow" />
            <BorrowRow
              pos={borrowPos}
              onRepay={() => setModal({ type: "repay", pos: borrowPos })}
            />
          </>
        )}
      </div>

      {modal?.type === "repay" && (
        <RepayModal
          position={modal.pos}
          isPending={repayMutation.isPending}
          onRepay={handleRepay}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.type === "leaveLp" && pool && poolData && (
        <LeaveModal
          pool={pool}
          poolData={poolData}
          onClose={() => setModal(null)}
        />
      )}
    </>
  );
}

import { parse_token_amount, token_amount_to_f64, type PoolWithIrm } from "@calma/wasm-lib";
import { useRepay } from "@/hooks/program/useRepay";
import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { useTokenBalance } from "@/hooks/useWalletBalances";
import { cn } from "@/lib/utils";
import type { Pool } from "@/types/pool";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { useMemo, useState } from "react";
import { HFBadge, HealthBadge } from "../common/Badge";
import { PositionActionButton } from "../common/PositionActionButton";
import { LeaveModal } from "../pool/LeaveModal";
// import { PutLpModal } from "./PutLpModal";
import { RepayModal, type RepayPosition } from "./RepayModal";
// import { TakeLpModal } from "./TakeLpModal";
import { type WithdrawPosition } from "./WithdrawModal";

// ─── Sub-components ──────────────────────────────────────────────────────────

function SectionLabel({ label }: { label: string }) {
  return (
    <div className="px-4 border-b border-surface-accent/8 bg-surface-accent/[0.015]">
      <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30">
        {label}
      </span>
    </div>
  );
}

function LendRow({
  pos,
  earned,
  health,
  // onClaimLp,
  // onPutLp,
  onRedeemLp,
}: {
  pos: WithdrawPosition;
  earned: number;
  health: number;
  // onClaimLp?: () => void;
  // onPutLp?: () => void;
  onRedeemLp?: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-10 gap-y-2 px-4 py-3.5 border-b border-surface-accent/8 last:border-none">
      {/* Asset */}
      <div className="flex items-center gap-2 min-w-[90px]">
        <img src={pos.icon} alt={pos.asset} className="h-6 w-6 rounded-full" />
        <p className="text-sm font-semibold text-surface-foreground">{pos.asset}</p>
      </div>

      {/* Supplied */}
      <div className="flex flex-col min-w-[80px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">Supplied</span>
        <span className="text-sm font-semibold tabular-nums text-surface-foreground">
          {pos.supplied.toLocaleString("en-US", { maximumFractionDigits: 4 })}
        </span>
      </div>

      {/* APY */}
      <div className="flex flex-col min-w-[60px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">APY</span>
        <span className="text-sm font-semibold tabular-nums text-success">
          {pos.apy.toFixed(2)}%
        </span>
      </div>

      <div className="flex flex-col min-w-[70px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">Earned</span>
        <span className="text-sm tabular-nums text-success">
          +${earned < 1 ? earned.toFixed(3) : earned.toFixed(2)}
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-xs ml-1 text-surface-foreground/35 mb-0.5">
          Health
        </span>
        <HealthBadge value={health} />
      </div>

      {/* Collateral */}
      <div className="flex flex-col min-w-[50px]">
        <span className="text-xs ml-1 text-surface-foreground/35 mb-0.5">
          Collateral
        </span>
        <span
          className={cn(
            "text-xs px-2 py-0.5 rounded-full font-medium w-fit",
            pos.collateralEnabled
              ? "bg-success/10 text-success"
              : "bg-surface-foreground/8 text-surface-foreground/35",
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
  ltv,
  liqPrice,
  healthFactor,
  onRepay,
}: {
  pos: RepayPosition;
  ltv: number | null;
  liqPrice: number | null;
  healthFactor: number | null;
  onRepay: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-10 gap-y-2 px-4 py-3.5 border-b border-surface-accent/8 last:border-none">
      {/* Borrowed asset (primary) */}
      <div className="flex items-center gap-2 min-w-[90px]">
        <img
          src={pos.borrowedIcon}
          alt={pos.borrowedAsset}
          className="h-6 w-6 rounded-full"
        />
        <p className="text-sm font-semibold text-surface-foreground">
          {pos.borrowedAsset}
        </p>
      </div>

      {/* Lend */}
      {/* <div className="flex items-center gap-2 min-w-[90px]">
        <div className="flex flex-col">
          <span className="text-xs text-surface-foreground/35 mb-0.5">Lended</span>

          <div className="flex items-center gap-1.5">
            <img
              src={pos.borrowedIcon}
              alt={pos.borrowedAsset}
              className="h-4 w-4 rounded-full"
            />
            <span className="text-xs text-surface-foreground/50">
              {pos.borrowedAsset}
            </span>
          </div>
        </div>
      </div> */}

      {/* Debt */}
      <div className="flex flex-col min-w-[80px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">Debt</span>
        <span className="text-sm font-semibold tabular-nums text-surface-foreground">
          {pos.debtAmount.toLocaleString("en-US", { maximumFractionDigits: 4 })}
        </span>
      </div>

      {/* Borrow APY */}
      <div className="flex flex-col min-w-[60px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">Borrow APY</span>
        <span className="text-sm font-semibold tabular-nums text-destructive">
          {pos.borrowAPY.toFixed(2)}%
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">LTV</span>
        <span className="text-sm tabular-nums text-surface-foreground/70">
          {ltv !== null ? `${ltv.toFixed(1)}%` : "N/A"}
        </span>
      </div>

      <div className="flex flex-col min-w-[70px]">
        <span className="text-xs text-surface-foreground/35 mb-0.5">Liq. Price</span>
        <span className="text-sm tabular-nums text-surface-foreground/70">
          {liqPrice !== null ? `$${liqPrice.toFixed(4)}` : "N/A"}
        </span>
      </div>

      <div className="flex flex-col min-w-[50px]">
        <span className="text-xs ml-1 text-surface-foreground/35 mb-0.5">HF</span>
        <HFBadge value={healthFactor} />
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
  poolData: PoolWithIrm;
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
  const { data: lendDecimals } = useMintDecimals(new PublicKey(poolData.lend_mint));
  const lpWalletBalance = useTokenBalance(new PublicKey(poolData.lp_mint));
  const lendWalletBalance = useTokenBalance(new PublicKey(poolData.lend_mint));

  const repayMutation = useRepay();

  // Compute on-chain debt as a human-readable number via WASM
  const debtUiAmount = useMemo(() => {
    if (!userPosition || !poolData || lendDecimals == null) return null;
    return token_amount_to_f64(poolData.debt_amount(userPosition) ?? 0n, lendDecimals);
  }, [userPosition, poolData, lendDecimals]);

  // Convert LP share balance → underlying lend tokens via on-chain exchange rate
  const suppliedLend = useMemo(() => {
    if (!lpWalletBalance || lendDecimals == null) return 0;
    const raw = poolData.lend_for_shares(lpWalletBalance.amount);
    return raw != null ? token_amount_to_f64(raw, lendDecimals) : 0;
  }, [lpWalletBalance, poolData, lendDecimals]);

  // LP wallet balance drives the Lend section (LP tokens are in user's wallet ATA)
  const hasLp = (lpWalletBalance?.uiAmount ?? 0) > 0;
  const hasDebt = userPosition != null && userPosition.has_debt();

  const supplyApyBps = poolData.supply_apy_bps();
  const lendEarned = +(suppliedLend * (supplyApyBps / 10_000) / 12).toFixed(4);
  const lendHealth = Math.max(0, Math.min(100, Math.round(100 - poolData.utilization_bps() / 100)));

  const ltvBps = userPosition ? poolData.ltv(userPosition) : undefined;
  const liqPriceBps = userPosition ? poolData.liq_price(userPosition) : undefined;
  const hfBps = userPosition ? poolData.health_factor(userPosition) : undefined;
  const borrowLtv = ltvBps != null ? ltvBps / 100 : null;
  const borrowLiqPrice = liqPriceBps != null ? liqPriceBps / 10_000 : null;
  const borrowHF = hfBps != null ? hfBps / 10_000 : null;

  const lendPos: WithdrawPosition | null = hasLp
    ? {
      asset: pool.lendSymbol,
      icon: pool.lendIcon,
      supplied: suppliedLend,
      apy: supplyApyBps / 100,
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
      rawDebtAmount: userPosition && poolData && lendDecimals != null
        ? (poolData.debt_amount(userPosition) ?? 0n).toString()
        : undefined,
      borrowAPY: pool.borrowAPY,
      walletBalance: lendWalletBalance?.uiAmount ?? undefined,
    }
    : null;

  async function handleRepay(amount: string, rawAmountStr?: string) {
    if (!poolData || !poolPubKey) return;
    // Use raw amount if provided (for max repayment), otherwise parse the UI amount
    const rawAmount = rawAmountStr
      ? new BN(rawAmountStr)
      : new BN((parse_token_amount(amount, lendDecimals ?? 6) ?? 0n).toString());
    await repayMutation.mutateAsync({
      pool: poolPubKey,
      lendMint: new PublicKey(poolData.lend_mint),
      amount: rawAmount,
    });
  }

  if (!connected) return null;
  if (positionLoading) {
    return (
      <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] px-4 py-6 flex items-center gap-2">
        <div className="h-1.5 w-1.5 rounded-full bg-surface-accent animate-pulse" />
        <span className="text-xs text-surface-foreground/30">
          Loading positions…
        </span>
      </div>
    );
  }
  if (!lendPos && !borrowPos) return null;

  return (
    <>
      <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] overflow-hidden">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-surface-accent/10">
          <div className="h-1.5 w-1.5 rounded-full bg-surface-accent" />
          <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/45">
            My Positions
          </span>
        </div>

        {lendPos && (
          <>
            <SectionLabel label="Lend" />
            <LendRow
              pos={lendPos}
              earned={lendEarned}
              health={lendHealth}
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
              ltv={borrowLtv}
              liqPrice={borrowLiqPrice}
              healthFactor={borrowHF}
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

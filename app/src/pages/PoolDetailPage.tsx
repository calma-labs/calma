import { BackButton } from "@/components/common/BackButton";
import { BorrowModal } from "@/components/pool/BorrowModal";
import { DepositModal } from "@/components/pool/DepositModal";
import { ParticipateModal } from "@/components/pool/ParticipateModal";
import { PoolHero, PoolStatsBar } from "@/components/pool/PoolHero";
import {
  UtilizationGauge,
  UtilizationInfoPanel,
} from "@/components/pool/UtilizationGauge";
import { BorrowApyChart } from "@/components/pool/charts/BorrowApyChart";
import { SupplyApyChart } from "@/components/pool/charts/SupplyApyChart";
import { TotalBorrowChart } from "@/components/pool/charts/TotalBorrowChart";
import { TotalSupplyChart } from "@/components/pool/charts/TotalSupplyChart";
import { PoolPositionPanel } from "@/components/portfolio/PoolPositionPanel";
import {
  WithdrawModal,
  type WithdrawPosition,
} from "@/components/portfolio/WithdrawModal";
import { useLendingAccount } from "@/hooks/program/useLendingAccount";
import { useUserPosition } from "@/hooks/program/useUserPosition";
import { useWithdraw } from "@/hooks/program/useWithdraw";
import { useMintDecimals } from "@/hooks/useMintDecimals";
import { poolDataToDisplayPool } from "@/lib/poolDisplay";
import { BN } from "@anchor-lang/core";
import { useWalletConnection } from "@solana/react-hooks";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router";

type ModalMode = "deposit" | "borrow" | "participate" | "withdraw";

export function PoolDetailPage() {
  const { address } = useParams<{ address: string }>();
  const navigate = useNavigate();
  const { connected, wallet } = useWalletConnection();
  const [modal, setModal] = useState<ModalMode | null>(null);

  const poolPubKey = useMemo(() => {
    if (!address) return null;
    try {
      return new PublicKey(address);
    } catch {
      return null;
    }
  }, [address]);

  const walletPubKey = useMemo(() => {
    if (!wallet) return null;
    try {
      return new PublicKey(wallet.account.publicKey);
    } catch {
      return null;
    }
  }, [wallet]);

  const { data: poolData, isLoading } = useLendingAccount(poolPubKey);
  const { data: userPosition } = useUserPosition(poolPubKey, walletPubKey);
  const { data: collateralDecimals } = useMintDecimals(
    poolData ? new PublicKey(poolData.collateral_mint) : null,
  );
  const { data: lendDecimals } = useMintDecimals(
    poolData ? new PublicKey(poolData.lend_mint) : null,
  );
  const withdrawMutation = useWithdraw();

  const pool = useMemo(
    () => (poolData && poolPubKey ? poolDataToDisplayPool(poolPubKey, poolData, lendDecimals ?? 6) : null),
    [poolData, poolPubKey, lendDecimals],
  );

  const withdrawPosition = useMemo<WithdrawPosition | null>(() => {
    if (!pool || !userPosition || userPosition.collateral_deposited === 0n)
      return null;
    const decimals = collateralDecimals ?? 9;
    return {
      asset: pool.collateralSymbol,
      icon: pool.collateralIcon,
      supplied: Number(userPosition.collateral_deposited) / 10 ** decimals,
      rawSupplied: userPosition.collateral_deposited.toString(),
      apy: pool.supplyAPY,
      collateralEnabled: true,
    };
  }, [pool, userPosition, collateralDecimals]);

  async function handleWithdraw(amount: number, rawAmountStr?: string) {
    if (!poolData || !walletPubKey || !poolPubKey) return;
    // Use raw amount if provided (for max withdrawal), otherwise calculate from UI amount
    const rawAmount = rawAmountStr ? new BN(rawAmountStr) : new BN(Math.floor(amount * 10 ** (collateralDecimals ?? 9)));
    const userTokenAccount = getAssociatedTokenAddressSync(
      new PublicKey(poolData.collateral_mint),
      walletPubKey,
    );
    await withdrawMutation.mutateAsync({
      pool: poolPubKey,
      collateralMint: new PublicKey(poolData.collateral_mint),
      userTokenAccount,
      amount: rawAmount,
    });
  }

  const chartSeed = useMemo(
    () => (address ? address.charCodeAt(0) + address.charCodeAt(1) : 42),
    [address],
  );

  if (!poolPubKey) {
    return (
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/" label="Back to markets" />
        <div className="flex flex-col items-center justify-center py-28 gap-4">
          <p className="text-[#efe0f7]/50 text-sm">Invalid pool address.</p>
          <button
            onClick={() => navigate("/")}
            className="text-xs text-[#c698e5] hover:underline"
          >
            ← Back to markets
          </button>
        </div>
      </div>
    );
  }

  if (isLoading || !pool) {
    return (
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/" label="Back to markets" />
        <div className="flex flex-col items-center justify-center py-28 gap-4">
          {isLoading ? (
            <p className="text-[#efe0f7]/30 text-sm">Loading pool…</p>
          ) : (
            <>
              <p className="text-[#efe0f7]/50 text-sm">Pool not found.</p>
              <button
                onClick={() => navigate("/")}
                className="text-xs text-[#c698e5] hover:underline"
              >
                ← Back to markets
              </button>
            </>
          )}
        </div>
      </div>
    );
  }

  return (
    <>
      {modal === "withdraw" && withdrawPosition && (
        <WithdrawModal
          position={withdrawPosition}
          isPending={withdrawMutation.isPending}
          onWithdraw={handleWithdraw}
          onClose={() => setModal(null)}
        />
      )}
      {modal === "deposit" && pool && poolData && (
        <DepositModal
          pool={pool}
          poolData={poolData}
          onClose={() => setModal(null)}
        />
      )}
      {modal === "borrow" && pool && poolData && (
        <BorrowModal
          pool={pool}
          poolData={poolData}
          onClose={() => setModal(null)}
        />
      )}
      {modal === "participate" && pool && poolData && (
        <ParticipateModal
          pool={pool}
          poolData={poolData}
          onClose={() => setModal(null)}
        />
      )}

      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/" label="Back to Markets" />

        <PoolHero
          pool={pool}
          isWalletConnected={connected}
          hasWithdrawPosition={!!withdrawPosition}
          onDeposit={() => setModal("deposit")}
          onWithdraw={() => setModal("withdraw")}
          onBorrow={() => setModal("borrow")}
          onLend={() => setModal("participate")}
        />

        <PoolStatsBar pool={pool} />

        <div className="mt-5">
          <PoolPositionPanel
            pool={pool}
            poolData={poolData!}
            connected={connected}
          />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-8">
          <TotalSupplyChart
            totalSupplied={pool.totalSupplied}
            seed={chartSeed}
          />
          <SupplyApyChart supplyApy={pool.supplyAPY} seed={chartSeed} />
        </div>

        <div className="mt-6 grid grid-cols-1 lg:grid-cols-4 gap-6 items-start">
          <div className="lg:col-span-3">
            <UtilizationGauge
              utilization={pool.utilization}
              totalSupplied={pool.totalSupplied}
              totalBorrowed={pool.totalBorrowed}
            />
          </div>
          <div className="lg:col-span-1 h-full">
            <UtilizationInfoPanel />
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-8">
          <TotalBorrowChart
            totalBorrowed={pool.totalBorrowed}
            seed={chartSeed}
          />
          <BorrowApyChart borrowApy={pool.borrowAPY} seed={chartSeed} />
        </div>
      </div>
    </>
  );
}

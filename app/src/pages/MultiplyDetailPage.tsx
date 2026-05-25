import { BackButton } from "@/components/common/BackButton";
import { LeverageModal } from "@/components/multiply/LeverageModal";
import { MultiplyAnalyticsCharts } from "@/components/multiply/MultiplyAnalyticsCharts";
import { MultiplyChart } from "@/components/multiply/MultiplyChart";
import { MultiplyHero } from "@/components/multiply/MultiplyHero";
import { MultiplyStatsGrid } from "@/components/multiply/MultiplyStatsGrid";
import { MultiplyPositionPanel } from "@/components/portfolio/MultiplyPositionPanel";
import { useLendingAccount } from "@/hooks/program/useLendingAccount";
import { poolDataToDisplayPool } from "@/lib/poolDisplay";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router";

export function MultiplyDetailPage() {
  // The :id param holds the pool's base-58 address (set by MultiplyTable navigation)
  const { id: address } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { connected } = useWalletConnection();
  const [isModalOpen, setIsModalOpen] = useState(false);

  const poolPubKey = useMemo(() => {
    if (!address) return null;
    try {
      return new PublicKey(address);
    } catch {
      return null;
    }
  }, [address]);

  const { data: poolData, isLoading } = useLendingAccount(poolPubKey);

  const pool = useMemo(
    () => (poolData && poolPubKey ? poolDataToDisplayPool(poolPubKey, poolData) : null),
    [poolData, poolPubKey],
  );

  const chartSeed = useMemo(
    () =>
      address ? address.charCodeAt(0) * 31 + (address.charCodeAt(1) ?? 0) : 42,
    [address],
  );

  if (isLoading || !pool || !poolData) {
    return (
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/multiply" label="Back to Multiply" />
        <div className="flex flex-col items-center justify-center py-28 gap-4">
          {isLoading ? (
            <p className="text-[#efe0f7]/30 text-sm">Loading strategy…</p>
          ) : (
            <>
              <p className="text-[#efe0f7]/50 text-sm">Strategy not found.</p>
              <button
                onClick={() => navigate("/multiply")}
                className="text-xs text-[#c698e5] hover:underline cursor-pointer"
              >
                ← Back to Multiply
              </button>
            </>
          )}
        </div>
      </div>
    );
  }

  return (
    <>
      {isModalOpen && (
        <LeverageModal
          pool={pool}
          poolData={poolData}
          onClose={() => setIsModalOpen(false)}
        />
      )}

      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/multiply" label="Back to Multiply" />

        <MultiplyHero
          pool={pool}
          isWalletConnected={connected}
          onOpenPosition={() => setIsModalOpen(true)}
        />

        <MultiplyStatsGrid pool={pool} />

        <div className="mt-5">
          <MultiplyPositionPanel
            pool={pool}
            poolData={poolData}
            connected={connected}
          />
        </div>

        {/* Price chart (2/3) + How it Works sidebar (1/3) */}
        <div className="mt-12 grid grid-cols-1 lg:grid-cols-3 gap-6 items-stretch">
          <div className="lg:col-span-2">
            <MultiplyChart pool={pool} seed={chartSeed} />
          </div>

          {/* How it Works — vertical stepper */}
          <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.02] px-5 py-6 flex flex-col">
            <p className="text-[10px] font-semibold uppercase tracking-widest text-[#efe0f7]/28 mb-6">
              How Multiply Works
            </p>

            <div className="flex flex-col flex-1">
              <div className="flex gap-3.5 flex-1">
                <div className="flex flex-col items-center">
                  <div className="flex-shrink-0 flex items-center justify-center h-6 w-6 rounded-full border border-[#c698e5]/35 bg-[#c698e5]/12 text-[9px] font-bold text-[#c698e5] leading-none">
                    1
                  </div>
                  <div className="w-px flex-1 bg-gradient-to-b from-[#c698e5]/20 to-transparent mt-2" />
                </div>
                <div className="pb-6 pt-0.5">
                  <p className="text-sm font-semibold text-[#efe0f7]/70 mb-1 leading-snug">
                    Deposit collateral
                  </p>
                  <p className="text-xs text-[#efe0f7]/35 leading-relaxed">
                    Supply {pool.collateralSymbol} as collateral into the pool.
                  </p>
                </div>
              </div>
              <div className="flex gap-3.5 flex-1">
                <div className="flex flex-col items-center">
                  <div className="flex-shrink-0 flex items-center justify-center h-6 w-6 rounded-full border border-[#c698e5]/35 bg-[#c698e5]/12 text-[9px] font-bold text-[#c698e5] leading-none">
                    2
                  </div>
                  <div className="w-px flex-1 bg-gradient-to-b from-[#c698e5]/20 to-transparent mt-2" />
                </div>
                <div className="pb-6 pt-0.5">
                  <p className="text-sm font-semibold text-[#efe0f7]/70 mb-1 leading-snug">
                    Borrow {pool.lendSymbol}
                  </p>
                  <p className="text-xs text-[#efe0f7]/35 leading-relaxed">
                    Protocol flash-borrows {pool.lendSymbol}, swaps to more{" "}
                    {pool.collateralSymbol} and deposits it.
                  </p>
                </div>
              </div>
              <div className="flex gap-3.5">
                <div className="flex-shrink-0 flex items-center justify-center h-6 w-6 rounded-full border border-[#c698e5]/35 bg-[#c698e5]/12 text-[9px] font-bold text-[#c698e5] leading-none">
                  3
                </div>
                <div className="pt-0.5">
                  <p className="text-sm font-semibold text-[#efe0f7]/70 mb-1 leading-snug">
                    Compounded exposure
                  </p>
                  <p className="text-xs text-[#efe0f7]/35 leading-relaxed">
                    One atomic transaction, up to {pool.ltv}% LTV. Net APY =
                    supply yield × leverage − borrow cost.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Analytics: Available Liquidity + Net APY over time */}
        <MultiplyAnalyticsCharts pool={pool} seed={chartSeed} />

        {!connected && (
          <p className="mt-6 text-center text-xs text-[#efe0f7]/30">
            Connect your wallet to open a leveraged position.
          </p>
        )}
      </div>
    </>
  );
}

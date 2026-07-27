import { StatCard } from "@/components/common/StatCard";
import { BorrowTable } from "@/components/portfolio/BorrowTable";
import { LendTable } from "@/components/portfolio/LendTable";
import { MultiplyPositionsTable } from "@/components/portfolio/MultiplyPositionsTable";
import { PortfolioChart } from "@/components/portfolio/PortfolioChart";
import { usePortfolioSummary } from "@/hooks/usePortfolio";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { Wallet } from "lucide-react";
import { useState } from "react";

const TABS = ["Lend", "Borrow", "Multiply"] as const;
type Tab = (typeof TABS)[number];

export function PortfolioPage() {
  const { connected } = useWalletConnection();
  const [activeTab, setActiveTab] = useState<Tab>("Lend");
  const { data: summary } = usePortfolioSummary(connected);

  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      {/* ── Header ── */}
      <div className="mb-8">
        <div className="flex items-center gap-2.5 mb-2">
          <Wallet className="h-5 w-5 text-surface-accent" />
          <h1 className="text-3xl font-semibold tracking-tight text-surface-foreground">
            Portfolio
          </h1>
        </div>
        <p className="text-sm text-surface-foreground/50 max-w-md">
          Track your active positions, earnings and borrowing health across all
          Calma strategies.
        </p>
      </div>

      {/* ── Wallet gate ── */}
      {!connected ? (
        <div className="flex flex-col items-center justify-center py-28 gap-6">
          <div className="rounded-full border border-surface-accent/20 bg-surface-accent/[0.06] p-5">
            <Wallet className="h-10 w-10 text-surface-accent/60" />
          </div>
          <div className="text-center">
            <p className="text-lg font-semibold text-surface-foreground/80 mb-1">
              Connect your wallet
            </p>
            <p className="text-sm text-surface-foreground/35 max-w-xs">
              Connect a wallet to view your active positions and portfolio
              performance.
            </p>
          </div>
        </div>
      ) : (
        <>
          {/* ── Stats ── */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
            <StatCard
              label="Net Portfolio Value"
              value={summary ? `$${summary.netValue.toLocaleString()}` : "—"}
              sub={
                summary
                  ? `${
                      summary.change30d >= 0 ? "+" : ""
                    }$${summary.change30d.toFixed(
                      0,
                    )} (${summary.change30dPct.toFixed(2)}%) 30d`
                  : undefined
              }
              positive={summary ? summary.change30d >= 0 : undefined}
            />
            <StatCard
              label="Total Supplied"
              value={
                summary ? `$${summary.totalSupplied.toLocaleString()}` : "—"
              }
              sub="Across all lend positions"
            />
            <StatCard
              label="Total Debt"
              value={summary ? `−$${summary.totalDebt.toLocaleString()}` : "—"}
              sub="Active borrows"
              positive={false}
            />
            <StatCard
              label="Leveraged Exposure"
              value={
                summary ? `$${summary.leveragedExposure.toLocaleString()}` : "—"
              }
              sub="Multiply positions"
            />
          </div>

          {/* ── Net Value Chart ── */}
          {summary && (summary.totalSupplied > 0 || summary.totalDebt > 0) && (
            <PortfolioChart
              history={summary.history}
              changePct30d={summary.change30dPct}
            />
          )}

          {/* ── Positions ── */}
          <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] overflow-hidden">
            {/* Tab bar */}
            <div className="flex items-center gap-1 border-b border-surface-accent/10 px-4 pt-3 pb-0">
              {TABS.map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={cn(
                    "relative cursor-pointer pb-3 px-3 text-sm font-medium transition-colors",
                    activeTab === tab
                      ? "text-surface-accent"
                      : "text-surface-foreground/40 hover:text-surface-foreground/70",
                  )}
                >
                  {tab}
                  {activeTab === tab && (
                    /* style-exception: 2px tab underline indicator requires sub-pixel height */
                    <span className="absolute bottom-0 left-0 right-0 h-[2px] rounded-full bg-surface-accent" />
                  )}
                </button>
              ))}
            </div>

            {/* Tab content */}
            {/* style-exception: 180px min-height prevents layout shift while tab content loads */}
            <div className="min-h-[180px]">
              {activeTab === "Lend" && <LendTable />}
              {activeTab === "Borrow" && <BorrowTable />}
              {activeTab === "Multiply" && <MultiplyPositionsTable />}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

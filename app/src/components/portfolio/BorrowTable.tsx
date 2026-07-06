import { TD, TH } from "@/components/common/tableStyles";
import { useBorrowPositions } from "@/hooks/usePortfolio";
import { cn } from "@/lib/utils";
import { ExternalLink } from "lucide-react";
import { useNavigate } from "react-router";
import { HFBadge } from "../common/Badge";
import { EmptyTableState } from "../common/EmptyTableState";

export function BorrowTable() {
  const { data: positions = [], isLoading } = useBorrowPositions();
  const navigate = useNavigate();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16 text-surface-foreground/30 text-sm">
        Loading positions…
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      {positions.length === 0 ? (
        <EmptyTableState />
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-surface-accent/10">
              <th className={TH}>Collateral</th>
              <th className={TH}>Borrowed</th>
              <th className={cn(TH, "text-right")}>Debt</th>
              <th className={cn(TH, "text-right")}>Borrow APY</th>
              <th className={cn(TH, "text-right")}>LTV</th>
              <th className={cn(TH, "text-right")}>Liq. Price</th>
              <th className={cn(TH, "text-right")}>Health Factor</th>
              <th className={cn(TH, "w-8")}></th>
            </tr>
          </thead>
          <tbody>
            {positions.map((pos, i) => (
              <tr
                key={pos.id}
                onClick={() => navigate(`/pool/${pos.poolId}`)}
                className={cn(
                  "border-b border-surface-accent/6 hover:bg-surface-accent/[0.04] cursor-pointer transition-colors",
                  i === positions.length - 1 && "border-none",
                )}
              >
                <td className={cn(TD, "w-full")}>
                  <div className="flex items-center gap-2.5">
                    <img
                      src={pos.collateralIcon}
                      alt={pos.collateralAsset}
                      className="h-6 w-6 rounded-full"
                    />
                    <span className="font-semibold text-surface-foreground">
                      {pos.collateralAsset}
                    </span>
                  </div>
                </td>
                <td className={TD}>
                  <div className="flex items-center gap-2.5">
                    <img
                      src={pos.borrowedIcon}
                      alt={pos.borrowedAsset}
                      className="h-6 w-6 rounded-full"
                    />
                    <span className="font-semibold text-surface-foreground">
                      {pos.borrowedAsset}
                    </span>
                  </div>
                </td>
                <td className={cn(TD, "text-right tabular-nums font-medium")}>
                  ${pos.debtAmount.toLocaleString()}
                </td>
                <td className={cn(TD, "text-right text-destructive font-semibold tabular-nums")}>
                  {pos.borrowAPY.toFixed(2)}%
                </td>
                <td className={cn(TD, "text-right tabular-nums")}>
                  {pos.ltv !== null ? `${pos.ltv.toFixed(1)}%` : "N/A"}
                </td>
                <td className={cn(TD, "text-right tabular-nums")}>
                  {pos.liqPrice !== null ? `$${pos.liqPrice.toFixed(4)}` : "N/A"}
                </td>
                <td className={cn(TD, "text-right")}>
                  <div className="flex justify-end">
                    <HFBadge value={pos.healthFactor} />
                  </div>
                </td>
                <td className={cn(TD, "text-right pr-5")}>
                  <ExternalLink className="h-3.5 w-3.5 text-surface-foreground/25" />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

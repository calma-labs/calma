import { TD, TH } from "@/components/common/tableStyles";
import { useMultiplyPositions } from "@/hooks/usePortfolio";
import { ExternalLink } from "lucide-react";
import { useNavigate } from "react-router";

export function MultiplyPositionsTable() {
  const { data: positions = [], isLoading } = useMultiplyPositions();
  const navigate = useNavigate();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16 text-[#efe0f7]/30 text-sm">
        Loading positions…
      </div>
    );
  }

  if (positions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-14 text-center">
        <p className="text-sm font-medium text-[#efe0f7]/50">
          No leveraged positions
        </p>
        <p className="text-xs text-[#efe0f7]/28 max-w-sm">
          Open a leveraged position via the{" "}
          <span className="text-[#c698e5]/70">Multiply</span> page to see it
          here.
        </p>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-[#c698e5]/10">
            <th className={TH}>Collateral</th>
            <th className={TH}>Debt Token</th>
            <th className={TH + " text-right"}>Multiplier</th>
            <th className={TH + " text-right"}>Net APY</th>
            <th className={TH + " text-right"}>Position Size</th>
            <th className={TH + " text-right"}>Liq. Price</th>
            <th className={TH + " w-4"}></th>
          </tr>
        </thead>
        <tbody>
          {positions.map((pos, i) => (
            <tr
              key={pos.id}
              onClick={() => navigate(`/multiply/${pos.poolId}`)}
              className={`border-b border-[#c698e5]/6 hover:bg-[#c698e5]/[0.04] cursor-pointer transition-colors ${
                i === positions.length - 1 ? "border-none" : ""
              }`}
            >
              <td className={TD + " w-full"}>
                <div className="flex items-center gap-2.5">
                  <img
                    src={pos.icon}
                    alt={pos.asset}
                    className="h-6 w-6 rounded-full"
                  />
                  <span className="font-semibold text-[#efe0f7]">
                    {pos.asset}
                  </span>
                </div>
              </td>
              <td className={TD + " text-[#efe0f7]/50 text-xs"}>
                <div className="flex items-center gap-2">
                  <img
                    src={pos.debtIcon}
                    alt={pos.debtAsset}
                    className="h-4 w-4 rounded-full"
                  />
                  <span>{pos.debtAsset}</span>
                </div>
              </td>
              <td
                className={
                  TD + " text-right tabular-nums font-bold text-[#c698e5]"
                }
              >
                {pos.multiplier.toFixed(1)}×
              </td>
              <td
                className={
                  TD + " text-right text-[#34d399] font-semibold tabular-nums"
                }
              >
                {pos.netAPY.toFixed(1)}%
              </td>
              <td className={TD + " text-right tabular-nums font-medium"}>
                ${pos.positionSize.toLocaleString()}
              </td>
              <td className={TD + " text-right tabular-nums text-[#efe0f7]/55"}>
                {pos.liqPrice !== null ? `$${pos.liqPrice.toFixed(4)}` : "N/A"}
              </td>
              <td className={TD + " text-right pr-5"}>
                <ExternalLink className="h-3.5 w-3.5 text-[#efe0f7]/25" />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

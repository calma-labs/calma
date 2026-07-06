import {
  AXIS_LINE,
  GRID_STYLE,
  LABEL_STYLE,
  TICK_STYLE,
  TOOLTIP_STYLE,
} from "@/lib/chartStyles";
import { formatLargeUSD } from "@/lib/formatters";
import type { PortfolioHistoryPoint } from "@/types/portfolio";
import { ArrowUpRight } from "lucide-react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

interface PortfolioChartProps {
  history: PortfolioHistoryPoint[];
  changePct30d: number;
}

function chartDomain(history: PortfolioHistoryPoint[]): [number, number] {
  if (history.length === 0) return [0, 100];
  const values = history.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const padding = Math.max((max - min) * 0.15, max * 0.05);
  return [Math.max(0, Math.floor(min - padding)), Math.ceil(max + padding)];
}

export function PortfolioChart({ history, changePct30d }: PortfolioChartProps) {
  const [yMin, yMax] = chartDomain(history);
  return (
    <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] overflow-hidden mb-6">
      <div className="px-5 pt-4 pb-3 border-b border-surface-accent/10 flex items-center justify-between">
        <div>
          <p className="text-xs font-semibold text-surface-foreground/60">
            Portfolio Net Value
          </p>
          <p className="text-xs text-surface-foreground/28 mt-0.5">90-day history</p>
        </div>
        <div className="flex items-center gap-1.5 text-xs font-semibold text-success">
          <ArrowUpRight className="h-3.5 w-3.5" />
          {changePct30d.toFixed(2)}% 30d
        </div>
      </div>
      <div className="py-3">
        <ResponsiveContainer width="100%" height={260}>
          <AreaChart
            data={history}
            margin={{ top: 8, right: 16, bottom: 8, left: 0 }}
          >
            <defs>
              {/* style-exception: recharts linearGradient stopColor requires hex color strings */}
              <linearGradient id="portfolioGrad" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#c698e5" stopOpacity={0.3} />
                <stop offset="95%" stopColor="#c698e5" stopOpacity={0.02} />
              </linearGradient>
            </defs>
            <CartesianGrid vertical={false} {...GRID_STYLE} />
            <XAxis
              dataKey="date"
              tick={TICK_STYLE}
              axisLine={AXIS_LINE}
              tickLine={false}
              interval={14}
            />
            <YAxis
              tickFormatter={(v: number) => formatLargeUSD(v)}
              tick={TICK_STYLE}
              axisLine={AXIS_LINE}
              tickLine={false}
              width={58}
              domain={[yMin, yMax]}
            />
            <Tooltip
              contentStyle={TOOLTIP_STYLE}
              labelStyle={LABEL_STYLE}
              formatter={(v) => [`$${Number(v).toLocaleString()}`, "Net Value"]}
            />
            {/* style-exception: recharts Area stroke and activeDot fill require hex color strings */}
            <Area
              type="monotone"
              dataKey="value"
              stroke="#c698e5"
              strokeWidth={2}
              fill="url(#portfolioGrad)"
              dot={false}
              activeDot={{ r: 4, fill: "#c698e5" }}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

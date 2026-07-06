import { ChartRangePicker } from "@/components/common/ChartRangePicker";
import {
  dateLabel,
  formatLargeUSD,
  seededRand,
  thinData,
} from "@/lib/formatters";

import { useMemo, useState } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

const RANGE_OPTIONS = ["30D", "5M", "1Y"] as const;
type Range = (typeof RANGE_OPTIONS)[number];
const RANGE_DAYS: Record<Range, number> = { "30D": 30, "5M": 150, "1Y": 365 };

function generateData(currentSupply: number, days: number, seed: number) {
  const data: { date: string; supply: number }[] = [];
  const now = new Date();
  const startFraction = 0.38 + (Math.sin(seed * 1.7) * 0.5 + 0.5) * 0.25;
  const startSupply = currentSupply * startFraction;

  for (let i = days; i >= 0; i--) {
    const date = new Date(now);
    date.setDate(date.getDate() - i);
    const t = (days - i) / days;
    const logistic = 1 / (1 + Math.exp(-7 * (t - 0.38)));
    const trend = startSupply + (currentSupply - startSupply) * logistic;
    const noise = seededRand(seed, i, 3.7) * 0.022;
    const value = Math.max(trend * (1 + noise), startSupply * 0.9);
    data.push({ date: dateLabel(date, days), supply: Math.round(value) });
  }
  return data;
}

interface TotalSupplyChartProps {
  totalSupplied: number;
  seed: number;
}

export function TotalSupplyChart({
  totalSupplied,
  seed,
}: TotalSupplyChartProps) {
  const [range, setRange] = useState<Range>("30D");

  const thinned = useMemo(
    () => thinData(generateData(totalSupplied, RANGE_DAYS[range], seed)),
    [totalSupplied, seed, range],
  );

  const minVal = Math.min(...thinned.map((d) => d.supply)) * 0.97;
  const maxVal = Math.max(...thinned.map((d) => d.supply)) * 1.02;

  const CustomTooltip = ({ active, payload, label }: any) => {
    if (!active || !payload?.length) return null;
    return (
      <div className="rounded-xl border border-surface-accent/20 bg-modal-bg px-3 py-2 text-xs">
        <p className="text-surface-foreground/40 mb-0.5">{label}</p>
        <p className="font-semibold text-surface-accent">
          {formatLargeUSD(payload[0].value)}
        </p>
      </div>
    );
  };

  return (
    <div className="mt-4 rounded-2xl border border-surface-accent/15 bg-surface-accent/[0.03] px-5 pt-5 pb-4">
      <div className="flex items-center justify-between mb-5">
        <div>
          <p className="text-sm font-semibold text-surface-foreground">Total Supply</p>
          <p className="text-xs text-surface-foreground/35 mt-0.5">
            Deposits over time
          </p>
        </div>
        <ChartRangePicker
          options={RANGE_OPTIONS}
          value={range}
          onChange={setRange}
        />
      </div>

      <ResponsiveContainer width="100%" height={180}>
        <AreaChart
          data={thinned}
          margin={{ top: 4, right: 4, left: -4, bottom: 0 }}
        >
          <defs>
            {/* style-exception: recharts SVG gradient requires hex color values */}
            <linearGradient id="totalSupplyGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#c698e5" stopOpacity={0.22} />
              <stop offset="100%" stopColor="#c698e5" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid
            strokeDasharray="3 3"
            stroke="rgba(198,152,229,0.07)"
            vertical={false}
          />
          <XAxis
            dataKey="date"
            tick={{ fill: "rgba(239,224,247,0.3)", fontSize: 10 }}
            tickLine={false}
            axisLine={false}
            interval="preserveStartEnd"
          />
          <YAxis
            domain={[minVal, maxVal]}
            tick={{ fill: "rgba(239,224,247,0.3)", fontSize: 10 }}
            tickLine={false}
            axisLine={false}
            tickFormatter={formatLargeUSD}
            width={52}
          />
          <Tooltip
            content={<CustomTooltip />}
            cursor={{ stroke: "rgba(198,152,229,0.2)", strokeWidth: 1 }}
          />
          {/* style-exception: recharts Area stroke requires hex color */}
          <Area
            type="monotone"
            dataKey="supply"
            stroke="#c698e5"
            strokeWidth={1.5}
            fill="url(#totalSupplyGrad)"
            dot={false}
            isAnimationActive
            animationDuration={700}
            animationEasing="ease-out"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

import { ChartRangePicker } from "@/components/common/ChartRangePicker";
import { dateLabel, seededRand, thinData } from "@/lib/formatters";
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

function generateData(baseApy: number, days: number, seed: number) {
  const data: { date: string; apy: number }[] = [];
  let value = baseApy;
  const now = new Date();

  for (let i = days; i >= 0; i--) {
    const date = new Date(now);
    date.setDate(date.getDate() - i);
    const x = seededRand(seed, i, 5.5);
    value = Math.max(0.5, Math.min(baseApy * 2.1, value + x * 0.32));
    data.push({
      date: dateLabel(date, days),
      apy: parseFloat(value.toFixed(2)),
    });
  }
  return data;
}

interface BorrowApyChartProps {
  borrowApy: number;
  seed: number;
}

export function BorrowApyChart({ borrowApy, seed }: BorrowApyChartProps) {
  const [range, setRange] = useState<Range>("30D");

  const thinned = useMemo(
    () => thinData(generateData(borrowApy, RANGE_DAYS[range], seed)),
    [borrowApy, seed, range],
  );

  const minVal = Math.floor(Math.min(...thinned.map((d) => d.apy)) - 0.5);
  const maxVal = Math.ceil(Math.max(...thinned.map((d) => d.apy)) + 0.5);

  const CustomTooltip = ({ active, payload, label }: any) => {
    if (!active || !payload?.length) return null;
    return (
      <div className="rounded-xl border border-surface-accent/20 bg-modal-bg px-3 py-2 text-xs">
        <p className="text-surface-foreground/40 mb-0.5">{label}</p>
        <p className="font-semibold text-warning">
          {payload[0].value.toFixed(2)}% APY
        </p>
      </div>
    );
  };

  return (
    <div className="rounded-2xl border border-surface-accent/15 bg-surface-accent/[0.03] px-5 pt-5 pb-4">
      <div className="flex items-center justify-between mb-5">
        <div>
          <p className="text-sm font-semibold text-surface-foreground">Borrow APY</p>
          <p className="text-xs text-surface-foreground/35 mt-0.5">
            Historical rate
          </p>
        </div>
        <ChartRangePicker
          options={RANGE_OPTIONS}
          value={range}
          onChange={setRange}
          activeClass="bg-warning/20 text-warning"
        />
      </div>

      <ResponsiveContainer width="100%" height={180}>
        <AreaChart
          data={thinned}
          margin={{ top: 4, right: 4, left: -28, bottom: 0 }}
        >
          <defs>
            {/* style-exception: recharts SVG gradient requires hex color values */}
            <linearGradient id="borrowApyGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#f0a854" stopOpacity={0.22} />
              <stop offset="100%" stopColor="#f0a854" stopOpacity={0} />
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
            tickFormatter={(v) => `${v}%`}
          />
          <Tooltip
            content={<CustomTooltip />}
            cursor={{ stroke: "rgba(240,168,84,0.2)", strokeWidth: 1 }}
          />
          {/* style-exception: recharts Area stroke requires hex color */}
          <Area
            type="monotone"
            dataKey="apy"
            stroke="#f0a854"
            strokeWidth={1.5}
            fill="url(#borrowApyGrad)"
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

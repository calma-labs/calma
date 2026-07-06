import { buildMultiplyMeta } from "@/hooks/useMultiply";
import {
  AXIS_LINE,
  GRID_STYLE,
  LABEL_STYLE,
  TICK_STYLE,
  TOOLTIP_STYLE,
} from "@/lib/chartStyles";
import { formatLargeUSD } from "@/lib/formatters";
import type { Pool } from "@/types/pool";
import { useMemo } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

// ─── Data generation ──────────────────────────────────────────────────────────

function shortDate(d: Date): string {
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function generateLiquidityData(
  seed: number,
  available: number,
  days = 180,
): { date: string; value: number }[] {
  const now = new Date();
  let val = available;
  return Array.from({ length: days + 1 }, (_, idx) => {
    const i = days - idx;
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    const s1 = Math.sin(seed * 1.3 + i * 0.05) * 0.022;
    const s2 = Math.sin(seed * 2.7 + i * 0.15) * 0.013;
    const s3 = Math.sin(seed * 0.4 + i * 0.007) * 0.007;
    val = Math.max(val * (1 + s1 + s2 + s3), available * 0.08);
    return { date: shortDate(d), value: Math.round(val) };
  });
}

type ApyRow = { date: string } & { [key: string]: number | string };

function generateApyMultiData(
  seed: number,
  baseApy: number,
  maxNetApy: number,
  maxMult: number,
  levels: number[],
  days = 180,
): ApyRow[] {
  const now = new Date();
  return Array.from({ length: days + 1 }, (_, idx) => {
    const i = days - idx;
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    const row: ApyRow = { date: shortDate(d) };
    levels.forEach((mult, li) => {
      const base =
        mult <= 1
          ? baseApy
          : baseApy +
            ((maxNetApy - baseApy) * (mult - 1)) / Math.max(maxMult - 1, 0.01);
      const volatility = 0.4 + (mult - 1) * 0.7;
      const noise =
        Math.sin(seed * 2.3 + i * 0.09 + mult * 1.7) * volatility +
        Math.sin(seed * 0.8 + i * 0.31 + mult * 0.5) * volatility * 0.4;
      const drift = Math.sin(seed * 1.1 + i * 0.02) * 0.25;
      row[`lev${li}`] = parseFloat(
        Math.max(base + noise + drift, 0.1).toFixed(3),
      );
    });
    return row;
  });
}

// ─── Constants ────────────────────────────────────────────────────────────────

// style-exception: chart line colors for multi-line recharts require hex values
const MULT_COLORS = [
  "#34d399",
  "#a3e635",
  "#f0a854",
  "#fb923c",
  "#d45677",
] as const;

// ─── Component ────────────────────────────────────────────────────────────────

interface Props {
  pool: Pool;
  seed: number;
}

export function MultiplyAnalyticsCharts({ pool, seed }: Props) {
  const meta = useMemo(() => buildMultiplyMeta(pool), [pool]);

  const multiplierLevels = useMemo(
    () =>
      Array.from({ length: 5 }, (_, i) =>
        parseFloat((1 + (i / 4) * (meta.maxMultiplier - 1)).toFixed(2)),
      ),
    [meta.maxMultiplier],
  );

  const liqData = useMemo(
    () => generateLiquidityData(seed, pool.availableLiquidity, 180),
    [seed, pool.availableLiquidity],
  );

  const apyData = useMemo(
    () =>
      generateApyMultiData(
        seed,
        pool.supplyAPY,
        meta.maxNetAPY,
        meta.maxMultiplier,
        multiplierLevels,
        180,
      ),
    [
      seed,
      pool.supplyAPY,
      meta.maxNetAPY,
      meta.maxMultiplier,
      multiplierLevels,
    ],
  );

  return (
    <div className="mt-6 grid grid-cols-1 md:grid-cols-2 gap-5">
      {/* ── Available liquidity ── */}
      <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] overflow-hidden">
        <div className="px-5 pt-4 pb-3 border-b border-surface-accent/10">
          <p className="text-xs font-semibold text-surface-foreground/60">
            Available Liquidity
          </p>
          <p className="text-xs text-surface-foreground/28 mt-0.5">
            180-day history
          </p>
        </div>

        <div className="py-3">
          <ResponsiveContainer width="100%" height={250}>
            <AreaChart
              data={liqData}
              margin={{ top: 8, right: 16, bottom: 8, left: 0 }}
            >
              <defs>
                {/* style-exception: recharts SVG gradient requires hex color values */}
                <linearGradient id="liqGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#c698e5" stopOpacity={0.28} />
                  <stop offset="95%" stopColor="#c698e5" stopOpacity={0.02} />
                </linearGradient>
              </defs>
              <CartesianGrid vertical={false} {...GRID_STYLE} />
              <XAxis
                dataKey="date"
                tick={TICK_STYLE}
                axisLine={AXIS_LINE}
                tickLine={false}
                interval={44}
              />
              <YAxis
                tickFormatter={(v: number) => formatLargeUSD(v)}
                tick={TICK_STYLE}
                axisLine={AXIS_LINE}
                tickLine={false}
                width={58}
              />
              <Tooltip
                contentStyle={TOOLTIP_STYLE}
                labelStyle={LABEL_STYLE}
                formatter={(v, _name) => [
                  formatLargeUSD(Number(v)),
                  "Available",
                ]}
              />
              {/* style-exception: recharts Area stroke requires hex color */}
              <Area
                type="monotone"
                dataKey="value"
                stroke="#c698e5"
                strokeWidth={2}
                fill="url(#liqGrad)"
                dot={false}
                activeDot={{ r: 4, fill: "#c698e5" }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* ── Net APY by multiplier level ── */}
      <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.02] overflow-hidden">
        <div className="px-5 pt-4 pb-3 border-b border-surface-accent/10">
          <p className="text-xs font-semibold text-surface-foreground/60">
            Net APY by Multiplier Level
          </p>
          <p className="text-xs text-surface-foreground/28 mt-0.5">
            180-day history
          </p>
        </div>

        <div className="pt-3 pb-1">
          <ResponsiveContainer width="100%" height={216}>
            <LineChart
              data={apyData}
              margin={{ top: 8, right: 0, bottom: 8, left: 16 }}
            >
              <CartesianGrid vertical={false} {...GRID_STYLE} />
              <XAxis
                dataKey="date"
                tick={TICK_STYLE}
                axisLine={AXIS_LINE}
                tickLine={false}
                interval={44}
              />
              <YAxis
                orientation="right"
                tickFormatter={(v: number) => `${v.toFixed(1)}%`}
                tick={TICK_STYLE}
                axisLine={AXIS_LINE}
                tickLine={false}
                width={44}
              />
              <Tooltip
                contentStyle={TOOLTIP_STYLE}
                labelStyle={LABEL_STYLE}
                formatter={(v, key) => {
                  const idx = parseInt(String(key).replace("lev", ""), 10);
                  return [
                    `${Number(v).toFixed(2)}%`,
                    `${multiplierLevels[idx]}×`,
                  ];
                }}
              />
              {/* style-exception: recharts Line strokes use MULT_COLORS hex array */}
              {multiplierLevels.map((_, i) => (
                <Line
                  key={i}
                  type="monotone"
                  dataKey={`lev${i}`}
                  stroke={MULT_COLORS[i]}
                  strokeWidth={1.5}
                  dot={false}
                  activeDot={{ r: 3, fill: MULT_COLORS[i] }}
                />
              ))}
            </LineChart>
          </ResponsiveContainer>

          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 pb-3 pt-1">
            {multiplierLevels.map((m, i) => (
              <div key={m} className="flex items-center gap-1.5">
                {/* style-exception: legend swatch color matches recharts line color */}
                <span
                  className="h-[3px] w-5 rounded-full inline-block"
                  style={{ backgroundColor: MULT_COLORS[i] }}
                />
                <span className="text-xs text-surface-foreground/35 tabular-nums">
                  {m}×
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

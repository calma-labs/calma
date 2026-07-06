import { getPoolMeta } from "@/config/poolRegistry";
import { useKlines } from "@/hooks/useKlines";
import { buildMultiplyMeta } from "@/hooks/useMultiply";
import { cn } from "@/lib/utils";
import type { Pool } from "@/types/pool";
import {
  CandlestickSeries,
  ColorType,
  createChart,
  type CandlestickData,
  type IChartApi,
  type Time,
} from "lightweight-charts";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  LabelList,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

type ChartTab = "price" | "apy";
type PriceRange = 30 | 90 | 180 | 365;

function generateOHLC(seed: number, days: number): CandlestickData<Time>[] {
  const data: CandlestickData<Time>[] = [];
  const now = new Date();
  const basePrice = 20 + Math.abs(Math.sin(seed * 3.7)) * 1980;
  let close = basePrice;

  for (let i = days; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    const time = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
      2,
      "0",
    )}-${String(d.getDate()).padStart(2, "0")}` as Time;

    const trend = Math.sin(seed * 1.1 + i * 0.03) * 0.008;
    const noise =
      Math.sin(seed * 2.3 + i * 0.27) * 0.018 +
      Math.sin(seed * 0.9 + i * 0.11) * 0.012;
    const spike =
      Math.abs(Math.sin(seed * 4.1 + i * 0.19)) > 0.93
        ? Math.sin(seed + i) * 0.045
        : 0;

    const changePercent = trend + noise + spike;
    const open = close;
    close = Math.max(open * (1 + changePercent), 0.01);

    const rangePct =
      0.008 +
      Math.abs(Math.sin(seed * 1.7 + i * 0.41)) * 0.022 +
      (Math.abs(spike) > 0 ? 0.02 : 0);
    const high = Math.max(open, close) * (1 + rangePct);
    const low = Math.min(open, close) * (1 - rangePct);

    data.push({ time, open, high, low, close });
  }
  return data;
}

function buildApyBars(
  supplyApy: number,
  maxNetApy: number,
  maxMult: number,
): { mult: number; apy: number }[] {
  const steps = Math.round(maxMult);
  // Derived net spread per unit of leverage from the meta max endpoint.
  // apy(N) = supplyApy + (N-1) * spread  →  always increasing, ends at maxNetApy.
  const spread =
    Math.max(maxNetApy - supplyApy, 0.5) / Math.max(maxMult - 1, 1);
  return Array.from({ length: steps }, (_, i) => {
    const mult = i + 1;
    const apy = supplyApy + (mult - 1) * spread;
    return { mult, apy: parseFloat(apy.toFixed(2)) };
  });
}

const PRICE_RANGE_LABELS: Record<PriceRange, string> = {
  30: "1M",
  90: "3M",
  180: "6M",
  365: "1Y",
};

interface MultiplyChartProps {
  pool: Pool;
  seed: number;
}

export function MultiplyChart({ pool, seed }: MultiplyChartProps) {
  const meta = useMemo(() => buildMultiplyMeta(pool), [pool]);
  const [tab, setTab] = useState<ChartTab>("price");
  const [priceRange, setPriceRange] = useState<PriceRange>(90);
  const chartRef = useRef<HTMLDivElement>(null);
  const chartApi = useRef<IChartApi | null>(null);

  const poolMeta = useMemo(() => getPoolMeta(pool.address), [pool.address]);
  const {
    data: liveKlines,
    isLoading: klinesLoading,
    isError: klinesError,
  } = useKlines(poolMeta.binancePerp);

  /**
   * Resolved dataset:
   * – live data when available
   * – mock ONLY when there is no binancePerp symbol OR the request failed
   *   (never while loading to avoid the mock→live flash)
   */
  const allKlines = useMemo<CandlestickData<Time>[] | null>(() => {
    if (liveKlines && liveKlines.length > 0) return liveKlines;
    // Still in-flight and we expect real data → return null so the chart waits
    if (poolMeta.binancePerp && klinesLoading) return null;
    // No symbol configured or request failed → use mock
    return generateOHLC(seed, 365);
  }, [liveKlines, klinesLoading, klinesError, poolMeta.binancePerp, seed]);

  useEffect(() => {
    if (tab !== "price") return;
    if (allKlines === null) return; // still loading – don't mount a chart yet
    const el = chartRef.current;
    if (!el) return;

    // style-exception: lightweight-charts requires raw color strings in its config API
    const chart = createChart(el, {
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "rgba(239,224,247,0.35)",
        fontFamily: "inherit",
        fontSize: 11,
      },
      grid: {
        vertLines: { color: "rgba(198,152,229,0.06)" },
        horzLines: { color: "rgba(198,152,229,0.06)" },
      },
      crosshair: {
        vertLine: {
          color: "rgba(198,152,229,0.3)",
          labelBackgroundColor: "#2d1040",
        },
        horzLine: {
          color: "rgba(198,152,229,0.3)",
          labelBackgroundColor: "#2d1040",
        },
      },
      rightPriceScale: {
        borderColor: "rgba(198,152,229,0.12)",
        scaleMargins: { top: 0.1, bottom: 0.1 },
      },
      timeScale: {
        borderColor: "rgba(198,152,229,0.12)",
        timeVisible: true,
        secondsVisible: false,
      },
      handleScroll: true,
      handleScale: true,
    });

    chartApi.current = chart;

    // style-exception: lightweight-charts CandlestickSeries requires hex colors
    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: "#34d399",
      downColor: "#d45677",
      borderUpColor: "#34d399",
      borderDownColor: "#d45677",
      wickUpColor: "rgba(52,211,153,0.5)",
      wickDownColor: "rgba(212,86,119,0.5)",
    });

    const sliced = allKlines.slice(Math.max(0, allKlines.length - priceRange));
    candleSeries.setData(sliced);
    chart.timeScale().fitContent();

    const handleResize = () => {
      if (el) chart.applyOptions({ width: el.clientWidth });
    };
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      chart.remove();
      chartApi.current = null;
    };
  }, [tab, priceRange, allKlines]);

  /** True while we are waiting for the first real response from Binance. */
  const isChartLoading = tab === "price" && allKlines === null;

  const apyBars = useMemo(
    () => buildApyBars(pool.supplyAPY, meta.maxNetAPY, meta.maxMultiplier),
    [pool.supplyAPY, meta.maxNetAPY, meta.maxMultiplier],
  );

  return (
    <div className="rounded-2xl border border-surface-accent/15 bg-surface-accent/[0.025] overflow-hidden flex flex-col h-full">
      <div className="flex items-center justify-between px-5 pt-4 pb-3 border-b border-surface-accent/10 flex-shrink-0">
        <div className="flex items-center gap-1 rounded-lg border border-surface-accent/12 bg-surface-accent/5 p-0.5">
          {(["price", "apy"] as ChartTab[]).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={cn(
                "rounded-md px-3.5 py-1.5 text-xs font-medium transition-all cursor-pointer capitalize",
                tab === t
                  ? "bg-surface-accent/20 text-surface-accent"
                  : "text-surface-foreground/35 hover:text-surface-foreground/70",
              )}
            >
              {t === "price" ? "Price" : "Net APY"}
            </button>
          ))}
        </div>

        {tab === "price" && (
          <div className="flex items-center gap-1">
            {([30, 90, 180, 365] as PriceRange[]).map((r) => (
              <button
                key={r}
                onClick={() => setPriceRange(r)}
                className={cn(
                  "rounded-md px-2.5 py-1 text-xs font-medium transition-all cursor-pointer",
                  priceRange === r
                    ? "bg-surface-accent/18 text-surface-accent"
                    : "text-surface-foreground/30 hover:text-surface-foreground/70",
                )}
              >
                {PRICE_RANGE_LABELS[r]}
              </button>
            ))}
          </div>
        )}
      </div>

      {tab === "price" ? (
        isChartLoading ? (
          <div className="flex-1 min-h-[430px] flex items-center justify-center">
            <div className="flex flex-col items-center gap-3">
              <div className="h-6 w-6 rounded-full border-2 border-surface-accent/40 border-t-surface-accent animate-spin" />
              <span className="text-xs text-surface-foreground/30">
                Loading chart…
              </span>
            </div>
          </div>
        ) : (
          <div ref={chartRef} className="flex-1 min-h-[430px]" />
        )
      ) : (
        <div className="flex-1 flex flex-col px-2 pt-3 pb-3 min-h-[430px]">
          <ResponsiveContainer width="100%" className="flex-1 min-h-0">
            <BarChart
              data={apyBars}
              margin={{ top: 28, right: 16, bottom: 20, left: 44 }}
            >
              <CartesianGrid
                strokeDasharray="3 3"
                stroke="rgba(198,152,229,0.07)"
                vertical={false}
              />
              <XAxis
                dataKey="mult"
                tickFormatter={(v) => `${v}×`}
                tick={{ fill: "rgba(239,224,247,0.3)", fontSize: 10 }}
                axisLine={{ stroke: "rgba(198,152,229,0.12)" }}
                tickLine={false}
              />
              <YAxis
                tickFormatter={(v: number) => `${v.toFixed(1)}%`}
                tick={{ fill: "rgba(239,224,247,0.28)", fontSize: 10 }}
                axisLine={{ stroke: "rgba(198,152,229,0.12)" }}
                tickLine={false}
                width={44}
              />
              {/* style-exception: recharts Tooltip contentStyle requires plain JS object with hex colors */}
              <Tooltip
                cursor={{ fill: "rgba(198,152,229,0.06)" }}
                contentStyle={{
                  background: "#1a0d24",
                  border: "1px solid rgba(198,152,229,0.2)",
                  borderRadius: "10px",
                  fontSize: "11px",
                  color: "#efe0f7",
                  boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
                }}
                labelStyle={{ color: "rgba(239,224,247,0.5)", marginBottom: 4 }}
                labelFormatter={(v) => `${v}× leverage`}
                formatter={(v, _name) => [
                  `${Number(v) >= 0 ? "+" : ""}${Number(v).toFixed(2)}%`,
                  "Net APY",
                ]}
              />
              <Bar dataKey="apy" radius={[4, 4, 0, 0]} maxBarSize={56}>
                <LabelList
                  dataKey="apy"
                  position="top"
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  formatter={(v: any) =>
                    `${Number(v) >= 0 ? "+" : ""}${Number(v).toFixed(1)}%`
                  }
                  style={{
                    fontSize: 9,
                    fontWeight: 600,
                    fill: "rgba(239,224,247,0.55)",
                  }}
                />
                {/* style-exception: recharts Cell fill color is computed from runtime bar data */}
                {apyBars.map((bar, i) => (
                  <Cell
                    key={i}
                    fill={bar.apy < 0 ? "#d45677" : "#34d399"}
                    fillOpacity={0.85}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>

          <div className="flex items-center justify-between mt-1 px-1">
            <div className="flex items-center gap-1.5">
              <div className="h-2 w-2 rounded-sm bg-success" />
              <span className="text-xs text-surface-foreground/40">
                1× = {pool.supplyAPY.toFixed(2)}% supply APY
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <div className="h-2 w-2 rounded-sm bg-surface-accent" />
              <span className="text-xs text-surface-foreground/40">
                {meta.maxMultiplier}× = {meta.maxNetAPY.toFixed(2)}% net APY
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

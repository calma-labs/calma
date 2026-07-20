import { RatePointsAccount } from "@calma/wasm-lib";
import { useMemo } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceArea,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

const POINTS = 126; // 0 %..125 % utilization in 1 % steps

export interface IrmPoint {
  /** Utilization in basis points (0..=10_000). */
  utilBps: number;
  /** Borrow rate in basis points. */
  rateBps: number;
}

interface IrmCurveChartProps {
  points: IrmPoint[];
}

export function IrmCurveChart({ points }: IrmCurveChartProps) {
  const data = useMemo(() => {
    const utils = new Uint16Array(points.map((p) => p.utilBps));
    const rates = new Uint32Array(points.map((p) => p.rateBps));
    const curve = RatePointsAccount.from_arrays(utils, rates);
    if (!curve) return [];
    const samples = Array.from({ length: POINTS }, (_, i) => ({
      util: i,
      rate: +(curve.rate_bps(BigInt(i * 100)) / 100).toFixed(2),
    }));
    curve.free();
    return samples;
  }, [points]);

  const maxRate = Math.max(...data.map((d) => d.rate), 1);
  const yMax = Math.ceil(maxRate * 1.12);

  const CustomTooltip = ({ active, payload }: any) => {
    if (!active || !payload?.length) return null;
    const util = payload[0].payload.util as number;
    const queued = util > 100;
    return (
      <div className="rounded-xl border border-[#c698e5]/20 bg-[#1a0d24] px-3 py-2 text-xs">
        <p className="text-[#efe0f7]/40 mb-0.5">
          {util}% utilization{queued ? " (queued)" : ""}
        </p>
        <p className={`font-semibold ${queued ? "text-[#f0a854]" : "text-[#c698e5]"}`}>
          {payload[0].value.toFixed(2)}% APY
        </p>
      </div>
    );
  };

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] px-5 pt-5 pb-4">
      <div className="mb-4">
        <p className="text-sm font-semibold text-[#efe0f7]">Rate Curve Preview</p>
        <p className="text-[11px] text-[#efe0f7]/35 mt-0.5">
          Borrow APY vs utilization
        </p>
      </div>

      <ResponsiveContainer width="100%" height={180}>
        <AreaChart
          data={data}
          margin={{ top: 4, right: 4, left: -28, bottom: 0 }}
        >
          <defs>
            <linearGradient id="irmCurveGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#c698e5" stopOpacity={0.25} />
              <stop offset="100%" stopColor="#c698e5" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid
            strokeDasharray="3 3"
            stroke="rgba(198,152,229,0.07)"
            vertical={false}
          />
          <XAxis
            dataKey="util"
            tick={{ fill: "rgba(239,224,247,0.3)", fontSize: 10 }}
            tickLine={false}
            axisLine={false}
            tickFormatter={(v) => `${v}%`}
            ticks={[0, 25, 50, 75, 100, 125]}
          />
          <YAxis
            domain={[0, yMax]}
            tick={{ fill: "rgba(239,224,247,0.3)", fontSize: 10 }}
            tickLine={false}
            axisLine={false}
            tickFormatter={(v) => `${v}%`}
          />
          <ReferenceArea
            x1={100}
            x2={125}
            fill="rgba(240,168,84,0.06)"
            stroke="none"
            label={{ value: "Queued", position: "insideTopLeft", fill: "rgba(240,168,84,0.45)", fontSize: 10 }}
          />
          <ReferenceLine
            x={100}
            stroke="rgba(240,168,84,0.35)"
            strokeDasharray="4 3"
          />
          <Tooltip
            content={<CustomTooltip />}
            cursor={{ stroke: "rgba(198,152,229,0.2)", strokeWidth: 1 }}
          />
          <Area
            type="monotone"
            dataKey="rate"
            stroke="#c698e5"
            strokeWidth={1.5}
            fill="url(#irmCurveGrad)"
            dot={false}
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

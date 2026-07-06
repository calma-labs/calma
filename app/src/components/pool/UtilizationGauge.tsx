import { formatRawTokens } from "@/lib/formatters";

interface UtilizationGaugeProps {
  utilization: number;
  totalSupplied: number;
  totalBorrowed: number;
}

// ─── SVG gauge geometry ───────────────────────────────────────────────────────
const GCX = 140; // centre x
const GCY = 128; // centre y
const GR = 90; // arc radius
const STW = 15; // stroke width
const ARC_START = 135; // degrees – 7-o'clock position
const ARC_SWEEP = 270; // total arc span

function pt(deg: number, r = GR) {
  const rad = (deg * Math.PI) / 180;
  return { x: GCX + r * Math.cos(rad), y: GCY + r * Math.sin(rad) };
}

function arcPath(startDeg: number, sweepDeg: number, r = GR): string {
  if (sweepDeg <= 0) return "";
  const clipped = Math.min(sweepDeg, 359.9);
  const s = pt(startDeg, r);
  const e = pt(startDeg + clipped, r);
  const large = clipped > 180 ? 1 : 0;
  return `M ${s.x.toFixed(2)} ${s.y.toFixed(
    2,
  )} A ${r} ${r} 0 ${large} 1 ${e.x.toFixed(2)} ${e.y.toFixed(2)}`;
}

// ─── Zone definitions ─────────────────────────────────────────────────────────
// This protocol's two-segment interest-rate curve keeps ≤95 % utilisation safe.
// Zone labels reflect that reality, not legacy 80 %-cap conventions.
export const ZONES = [
  {
    from: 0,
    to: 60,
    color: "#475569",
    label: "Idle",
    sub: "0 – 60 %",
    tip: "Capital underdeployed",
    desc: "Low demand. Capital sits unused in the vault.",
  },
  {
    from: 60,
    to: 85,
    color: "#38bdf8",
    label: "Efficient",
    sub: "60 – 85 %",
    tip: "Good utilisation",
    desc: "Healthy activity. Rates begin rising to attract more liquidity.",
  },
  {
    from: 85,
    to: 95,
    color: "#34d399",
    label: "Optimal",
    sub: "85 – 95 %",
    tip: "Curve-safe peak zone",
    desc: "Target zone. The curve keeps rates stable and economics balanced.",
  },
  {
    from: 95,
    to: 100,
    color: "#f59e0b",
    label: "Near Max",
    sub: "95 – 100 %",
    tip: "High rate pressure",
    desc: "Rate spikes sharply — incentivising new supply or repayments.",
  },
  {
    from: 100,
    to: Infinity,
    color: "#ef4444",
    label: "Over-utilized",
    sub: "> 100 %",
    tip: "Queue active",
    desc: "Withdrawal queue engaged. Protocol operating in over-utilization mode.",
  },
] as const;

export type Zone = (typeof ZONES)[number];

function zoneForPct(pct: number): Zone {
  return ([...ZONES].reverse().find((z) => pct >= z.from) ?? ZONES[0]) as Zone;
}

export function UtilizationGauge({
  utilization,
  totalSupplied,
  totalBorrowed,
}: UtilizationGaugeProps) {
  const pct = utilization;
  const zone = zoneForPct(pct);
  const available = totalSupplied - totalBorrowed;
  // Arc is capped at full sweep visually; the real value is shown in the centre text
  const arcPct = Math.min(100, Math.max(0, pct));
  const fillSweep = (arcPct / 100) * ARC_SWEEP;
  const tipPoint = pt(ARC_START + fillSweep);

  return (
    <div className="rounded-2xl border border-surface-accent/15 bg-surface-accent/[0.03] px-6 py-5">
      {/* ── Header ─────────────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between mb-1">
        <div>
          <p className="text-sm font-semibold text-surface-foreground">
            Pool Utilization
          </p>
          <p className="text-xs text-surface-foreground/35 mt-0.5">
            Curve-optimized capital efficiency
          </p>
        </div>
        {/* style-exception: zone color is computed at runtime from utilization data */}
        <span
          className="inline-flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider px-3 py-1 rounded-full border transition-colors duration-500"
          style={{
            color: zone.color,
            borderColor: zone.color + "40",
            backgroundColor: zone.color + "14",
          }}
        >
          {/* style-exception: zone dot color is dynamic */}
          <span
            className="h-1.5 w-1.5 rounded-full animate-pulse"
            style={{ backgroundColor: zone.color }}
          />
          {zone.label}
        </span>
      </div>

      {/* ── Arc Gauge (SVG) ─────────────────────────────────────────────────── */}
      <div className="flex justify-center">
        <svg
          width="280"
          height="250"
          viewBox="0 0 280 250"
          aria-label={`Utilization gauge: ${pct.toFixed(1)}%`}
        >
          <defs>
            {/* Glow filter for the active fill arc */}
            <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
              <feGaussianBlur stdDeviation="4" result="blur" />
              <feMerge>
                <feMergeNode in="blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          {/* Background track */}
          <path
            d={arcPath(ARC_START, ARC_SWEEP)}
            fill="none"
            stroke="#c698e5"
            strokeOpacity={0.08}
            strokeWidth={STW}
            strokeLinecap="round"
          />

          {/* Zone tint segments — style-exception: colors are data-driven from ZONES array */}
          {ZONES.map((z) => {
            const zStart = ARC_START + (z.from / 100) * ARC_SWEEP;
            const zSweep = ((z.to - z.from) / 100) * ARC_SWEEP;
            return (
              <path
                key={z.label}
                d={arcPath(zStart, zSweep)}
                fill="none"
                stroke={z.color}
                strokeOpacity={0.15}
                strokeWidth={STW}
              />
            );
          })}

          {/* Active fill arc — style-exception: fill color is runtime zone data */}
          {pct > 0 && (
            <path
              d={arcPath(ARC_START, fillSweep)}
              fill="none"
              stroke={zone.color}
              strokeWidth={STW}
              strokeLinecap="round"
              filter="url(#glow)"
            />
          )}

          {/* Zone divider ticks at 60 %, 85 %, 95 % */}
          {([60, 85, 95] as const).map((threshold) => {
            const tickDeg = ARC_START + (threshold / 100) * ARC_SWEEP;
            const inner = pt(tickDeg, GR - STW / 2 - 3);
            const outer = pt(tickDeg, GR + STW / 2 + 3);
            return (
              <line
                key={threshold}
                x1={inner.x}
                y1={inner.y}
                x2={outer.x}
                y2={outer.y}
                stroke="#1a0d24"
                strokeWidth={2.5}
              />
            );
          })}

          {/* Tip dot — style-exception: fill color is runtime zone data */}
          {pct > 0 && arcPct < 100 && (
            <circle
              cx={tipPoint.x}
              cy={tipPoint.y}
              r={5}
              fill={zone.color}
              filter="url(#glow)"
            />
          )}

          {/* ── Outer threshold labels — style-exception: fill colors are zone data */}
          {(() => {
            const labels: Array<{
              pct: number;
              color: string;
              anchor: "start" | "middle" | "end";
            }> = [
                { pct: 0, color: "#efe0f7", anchor: "middle" },
                { pct: 60, color: "#38bdf8", anchor: "middle" },
                { pct: 85, color: "#34d399", anchor: "middle" },
                { pct: 95, color: "#f59e0b", anchor: "middle" },
                { pct: 100, color: "#efe0f7", anchor: "middle" },
              ];
            return labels.map(({ pct: lp, color, anchor }) => {
              const pos = pt(
                ARC_START + (lp / 100) * ARC_SWEEP,
                GR + STW / 2 + 14,
              );
              return (
                <text
                  key={lp}
                  x={pos.x}
                  y={pos.y}
                  textAnchor={anchor}
                  dominantBaseline="middle"
                  fill={color}
                  fillOpacity={0.55}
                  fontSize={9}
                  fontWeight={600}
                  fontFamily="monospace"
                >
                  {lp === 0 ? "0%" : lp === 100 ? "100%" : `${lp}%`}
                </text>
              );
            });
          })()}

          {/* ── Centre readout — style-exception: fill color is runtime zone data */}
          <text
            x={GCX}
            y={GCY - 14}
            textAnchor="middle"
            fill={zone.color}
            fontSize={38}
            fontWeight={700}
            fontFamily="'Courier New', monospace"
          >
            {pct.toFixed(1)}%
          </text>
          <text
            x={GCX}
            y={GCY + 16}
            textAnchor="middle"
            fill={zone.color}
            fillOpacity={0.65}
            fontSize={11}
            fontWeight={600}
            letterSpacing={1}
          >
            {zone.sub}
          </text>
          <text
            x={GCX}
            y={GCY + 34}
            textAnchor="middle"
            fill="#efe0f7"
            fillOpacity={0.3}
            fontSize={9.5}
          >
            {zone.tip}
          </text>
        </svg>
      </div>

      {/* ── Zone legend ─────────────────────────────────────────────────────── */}
      <div className="flex justify-center gap-4 mb-4">
        {ZONES.map((z) => (
          <div key={z.label} className="flex items-center gap-1.5">
            {/* style-exception: zone dot color is runtime data */}
            <span
              className="h-2 w-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: z.color }}
            />
            {/* style-exception: zone label color is runtime data */}
            <span
              className="text-xs font-semibold uppercase tracking-wide"
              style={{ color: zone.label === z.label ? z.color : "#efe0f740" }}
            >
              {z.label}
            </span>
          </div>
        ))}
      </div>

      {/* ── Metrics strip ───────────────────────────────────────────────────── */}
      <div className="grid grid-cols-3 gap-2 border-t border-surface-accent/10 pt-4">
        <div className="flex flex-col gap-1">
          <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30">
            Total Supply
          </span>
          <span className="text-sm font-semibold text-surface-accent tabular-nums">
            {formatRawTokens(totalSupplied)}
          </span>
          <div className="h-0.5 rounded-full bg-surface-accent/20 mt-0.5">
            <div className="h-full w-full rounded-full bg-surface-accent" />
          </div>
        </div>

        <div className="flex flex-col gap-1 items-center">
          <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30">
            Borrowed
          </span>
          {/* style-exception: text color is runtime zone data */}
          <span
            className="text-sm font-semibold tabular-nums"
            style={{ color: zone.color }}
          >
            {formatRawTokens(totalBorrowed)}
          </span>
          <div className="h-0.5 rounded-full bg-surface-accent/10 mt-0.5 w-full overflow-hidden">
            {/* style-exception: bar width and color are computed from runtime utilization + zone data */}
            <div
              className="h-full rounded-full transition-all duration-700"
              style={{ width: `${Math.min(100, Math.max(0, pct))}%`, backgroundColor: zone.color }}
            />
          </div>
        </div>

        <div className="flex flex-col gap-1 items-end">
          <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/30">
            Available
          </span>
          <span className="text-sm font-medium text-surface-foreground/55 tabular-nums">
            {formatRawTokens(available)}
          </span>
          <div className="h-0.5 rounded-full bg-surface-foreground/8 mt-0.5 w-full overflow-hidden">
            {/* style-exception: bar width is computed from runtime utilization */}
            <div
              className="h-full rounded-full bg-surface-foreground/30 transition-all duration-700"
              style={{ width: `${Math.max(0, 100 - pct)}%` }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Side-panel companion component ──────────────────────────────────────────
export function UtilizationInfoPanel() {
  return (
    <div className="rounded-2xl border border-success/18 bg-success/[0.03] px-5 py-8 flex flex-col gap-5 h-full">
      {/* Title */}
      <div>
        <div className="flex items-center gap-2 mb-2.5">
          <div className="h-6 w-6 rounded-lg bg-success/15 flex items-center justify-center flex-shrink-0">
            <svg
              className="h-3.5 w-3.5 text-success"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
            >
              <path d="M2 12 L5 8 L8 10 L11 5 L14 3" />
            </svg>
          </div>
          <p className="text-sm font-semibold text-surface-foreground">
            Curve-Optimized
          </p>
        </div>
        <p className="text-xs text-surface-foreground/50 leading-relaxed">
          This pool's two-segment interest-rate curve dynamically adjusts
          borrowing costs — enabling{" "}
          <span className="text-success font-semibold">up to 95 %</span> safe
          utilization versus the ~80 % ceiling of traditional protocols.
        </p>
      </div>

      {/* Zone breakdown */}
      <div>
        <p className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/25 mb-3">
          Rate zones
        </p>
        <div className="space-y-3">
          {ZONES.map((z) => (
            <div key={z.label} className="flex items-start gap-2.5">
              {/* style-exception: zone dot color is runtime data */}
              <div
                className="mt-0.5 h-2 w-2 rounded-full flex-shrink-0"
                style={{ backgroundColor: z.color }}
              />
              <div>
                <div className="flex items-baseline gap-1.5">
                  {/* style-exception: zone label color is runtime data */}
                  <span
                    className="text-xs font-bold uppercase tracking-wide"
                    style={{ color: z.color }}
                  >
                    {z.label}
                  </span>
                  <span className="text-xs text-surface-foreground/30 font-medium">
                    {z.sub}
                  </span>
                </div>
                <p className="text-xs text-surface-foreground/45 leading-snug mt-0.5">
                  {z.desc}
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

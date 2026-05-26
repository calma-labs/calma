/** Shared badge components used across the portfolio feature */

export function HealthBadge({ value }: { value: number }) {
  const color =
    value <= 95
      ? "text-[#34d399] bg-[#34d399]/10"
      : value <= 99
      ? "text-[#f0a854] bg-[#f0a854]/10"
      : "text-[#d45677] bg-[#d45677]/10";
  return (
    <span
      className={`px-2 py-0.5 rounded-full text-[10px] font-semibold tabular-nums ${color}`}
    >
      {value}%
    </span>
  );
}

export function HFBadge({ value }: { value: number | null }) {
  if (value === null) {
    return (
      <span className="w-fit px-2 py-0.5 rounded-full text-[10px] font-semibold tabular-nums text-[#efe0f7]/30 bg-[#efe0f7]/5">
        N/A
      </span>
    );
  }
  const color =
    value >= 2.5
      ? "text-[#34d399] bg-[#34d399]/10"
      : value >= 1.5
      ? "text-[#f0a854] bg-[#f0a854]/10"
      : "text-[#d45677] bg-[#d45677]/10";
  return (
    <span
      className={`w-fit px-2 py-0.5 rounded-full text-[10px] font-semibold tabular-nums ${color}`}
    >
      {value.toFixed(2)}
    </span>
  );
}

import { BackButton } from "@/components/common/BackButton";
import { IrmCurveChart, type IrmPoint } from "@/components/pool/charts/IrmCurveChart";
import { FeedPairPanel } from "@/components/pool/FeedPairPanel";
import {
  useCreateLendingPool,
  type CreatePoolResult,
} from "@/hooks/program/useCreateLendingPool";
import { getTokenOptions } from "@/lib/tokenRegistry";
import { TokenSelect } from "@/components/ui/token-select";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import {
  CheckCircle2,
  Coins,
  Copy,
  ExternalLink,
  Loader2,
  Plus,
  Radio,
  Settings2,
  ShieldCheck,
} from "lucide-react";
import { useMemo, useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router";

// ─── helpers ──────────────────────────────────────────────────────────────────

function fieldClass(error: boolean) {
  return cn(
    "w-full rounded-xl border bg-surface-accent/[0.04] px-4 py-2.5 text-sm text-surface-foreground",
    "placeholder:text-surface-foreground/25 transition-colors duration-150",
    "focus:bg-surface-accent/[0.07] focus:border-surface-accent/50 focus-visible:ring-2 focus-visible:ring-ring",
    error
      ? "border-destructive/60 focus:border-destructive"
      : "border-surface-accent/20 hover:border-surface-accent/35",
  );
}

// ─── sub-components ───────────────────────────────────────────────────────────

interface FieldProps {
  id: string;
  label: string;
  hint?: string;
  error?: string;
  children: React.ReactNode;
}

function Field({ id, label, hint, error, children }: FieldProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <label
        htmlFor={id}
        className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/45"
      >
        {label}
      </label>
      {children}
      {hint && !error && (
        <p className="text-xs text-surface-foreground/30">{hint}</p>
      )}
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

interface NumberInputProps {
  id: string;
  value: string;
  onChange: (v: string) => void;
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
  hasError?: boolean;
}

function NumberInput({
  id,
  value,
  onChange,
  min,
  max,
  step = 1,
  placeholder,
  hasError = false,
}: NumberInputProps) {
  return (
    <input
      id={id}
      type="number"
      min={min}
      max={max}
      step={step}
      value={value}
      placeholder={placeholder}
      onChange={(e: ChangeEvent<HTMLInputElement>) => onChange(e.target.value)}
      className={fieldClass(hasError)}
    />
  );
}

// ─── section wrapper ──────────────────────────────────────────────────────────

function Section({
  title,
  icon,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-surface-accent/15 text-surface-accent">
          {icon}
        </span>
        <h2 className="text-sm font-semibold text-surface-foreground/80">{title}</h2>
      </div>
      <div className="flex flex-col gap-5">{children}</div>
    </div>
  );
}

// ─── address row (shown after deployment) ────────────────────────────────────

function AddressRow({ label, value }: { label: string; value: string }) {
  function copy() {
    navigator.clipboard.writeText(value);
  }

  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-semibold uppercase tracking-wider text-surface-foreground/35">
        {label}
      </span>
      <div className="flex items-center gap-2">
        <span className="flex-1 font-mono text-xs text-surface-foreground/80 break-all">
          {value}
        </span>
        <button
          type="button"
          onClick={copy}
          title="Copy to clipboard"
          className="shrink-0 rounded-lg p-1.5 text-surface-foreground/40 hover:bg-surface-accent/10 hover:text-surface-accent transition-colors cursor-pointer"
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}

// ─── page ─────────────────────────────────────────────────────────────────────

interface KinkPoint {
  /** Utilization in percent (0..150). */
  util: string;
  /** Rate in basis points. */
  rate: string;
}

interface FormState {
  ltvPercent: string;
  kinkPoints: KinkPoint[];
}

// (0%, 50 bps) → (95%, 450 bps) → (100%, 1000 bps) — matches the on-chain DEFAULT_POINTS.
const DEFAULT_KINK_POINTS: KinkPoint[] = [
  { util: "0", rate: "50" },
  { util: "95", rate: "450" },
  { util: "100", rate: "1000" },
];

const DEFAULT_FORM: FormState = {
  ltvPercent: "97",
  kinkPoints: DEFAULT_KINK_POINTS,
};

/** Convert a form KinkPoint (utilization %, rate bps) to on-chain bps. */
function toIrmPoint(kp: KinkPoint): IrmPoint {
  return {
    utilBps: Math.round(Number(kp.util) * 100),
    rateBps: Math.round(Number(kp.rate)),
  };
}

/** Validate the point list matches on-chain invariants. */
function pointsValid(points: KinkPoint[]): boolean {
  if (points.length < 2 || points.length > 4) return false;
  const bpsPoints = points.map(toIrmPoint);
  if (!Number.isFinite(bpsPoints[0].utilBps) || bpsPoints[0].utilBps !== 0) return false;
  for (let i = 1; i < bpsPoints.length; i++) {
    if (!Number.isFinite(bpsPoints[i].utilBps)) return false;
    if (bpsPoints[i].utilBps <= bpsPoints[i - 1].utilBps) return false;
  }
  return bpsPoints.every((p) => Number.isFinite(p.rateBps) && p.rateBps >= 0);
}

function validate(form: FormState): { ltvPercent?: string; kinkPoints?: string } {
  const errors: { ltvPercent?: string; kinkPoints?: string } = {};
  const ltv = Number(form.ltvPercent);
  if (form.ltvPercent === "" || isNaN(ltv) || ltv <= 0 || ltv > 100)
    errors.ltvPercent = "Must be between 1 and 100";
  if (!pointsValid(form.kinkPoints))
    errors.kinkPoints =
      "Need 2–4 points; first utilization must be 0 and utilizations strictly increasing.";
  return errors;
}

const TOKEN_OPTIONS = getTokenOptions();

export function CreatePoolPage() {
  const navigate = useNavigate();
  const { connected } = useWalletConnection();

  const [form, setForm] = useState<FormState>(DEFAULT_FORM);
  const [submitAttempted, setSubmitAttempted] = useState(false);
  const [result, setResult] = useState<CreatePoolResult | null>(null);
  const [lendAddr, setLendAddr] = useState("");
  const [collateralAddr, setCollateralAddr] = useState("");

  const errors = validate(form);
  const hasErrors = Object.keys(errors).length > 0;
  const mintsReady = !!lendAddr && !!collateralAddr && lendAddr !== collateralAddr;
  const canSubmit = connected && mintsReady;

  const collateralSymbol = useMemo(
    () => TOKEN_OPTIONS.find((o) => o.address === collateralAddr)?.symbol ?? "",
    [collateralAddr],
  );
  const lendSymbol = useMemo(
    () => TOKEN_OPTIONS.find((o) => o.address === lendAddr)?.symbol ?? "",
    [lendAddr],
  );
  const collateralMintPk = useMemo(
    () => (collateralAddr ? new PublicKey(collateralAddr) : null),
    [collateralAddr],
  );
  const lendMintPk = useMemo(
    () => (lendAddr ? new PublicKey(lendAddr) : null),
    [lendAddr],
  );

  const { mutateAsync, isPending } = useCreateLendingPool({
    onCreated: (r) => setResult(r),
  });

  function setKinkPoint(index: number, field: keyof KinkPoint, value: string) {
    setForm((prev) => ({
      ...prev,
      kinkPoints: prev.kinkPoints.map((p, i) => (i === index ? { ...p, [field]: value } : p)),
    }));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitAttempted(true);
    if (hasErrors || !canSubmit) return;

    await mutateAsync({
      collateralMint: new PublicKey(collateralAddr),
      lendMint: new PublicKey(lendAddr),
      ltvPercent: Number(form.ltvPercent),
      ratePoints: form.kinkPoints.map(toIrmPoint),
    });
  }

  // ── success screen ────────────────────────────────────────────────────────
  if (result) {
    return (
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/markets" label="Back to markets" />

        <div className="flex w-2/3 mx-auto flex-col justify-center mt-8">
          <div className="mb-8 flex items-center gap-3">
            <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-success/15 text-success">
              <CheckCircle2 className="h-5 w-5" />
            </span>
            <div>
              <h1 className="text-3xl font-semibold tracking-tight text-surface-foreground">
                Pool Deployed
              </h1>
              <p className="text-sm text-surface-foreground/50 mt-0.5">
                Save these addresses — they were generated automatically.
              </p>
            </div>
          </div>

          <div className="rounded-2xl border border-surface-accent/12 bg-surface-accent/[0.025] p-6 flex flex-col gap-4">
            <AddressRow
              label="Pool Address"
              value={result.poolAddress.toBase58()}
            />
            <div className="border-t border-surface-accent/10" />
            <AddressRow
              label="Collateral Mint"
              value={result.collateralMint.toBase58()}
            />
            <AddressRow label="Lend Mint" value={result.lendMint.toBase58()} />
          </div>

          <div className="flex gap-3 mt-6">
            {/* style-exception: glow shadow requires exact rgba for surface-accent color */}
            <button
              onClick={() => navigate(`/pool/${result.poolAddress.toBase58()}`)}
              className={cn(
                "flex items-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold transition-all duration-200",
                "bg-surface-accent text-surface shadow-[0_0_20px_rgba(198,152,229,0.30)]",
                "hover:brightness-110 hover:shadow-[0_0_28px_rgba(198,152,229,0.45)] active:scale-95 cursor-pointer",
              )}
            >
              <ExternalLink className="h-4 w-4" />
              View Pool
            </button>
            <button
              onClick={() => {
                setResult(null);
                setForm(DEFAULT_FORM);
                setSubmitAttempted(false);
              }}
              className="flex items-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold transition-all duration-200 border border-surface-accent/25 bg-surface-accent/8 text-surface-accent hover:border-surface-accent/50 hover:bg-surface-accent/15 active:scale-95 cursor-pointer"
            >
              <Plus className="h-4 w-4" />
              Create Another
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── form ──────────────────────────────────────────────────────────────────
  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      <BackButton to="/markets" label="Back to markets" />

      <div className="flex w-2/3 mx-auto flex-col justify-center mt-8">
        {/* Header */}
        <div className="mb-8">
          <div className="flex items-center gap-2.5 mb-2">
            <Plus className="h-5 w-5 text-surface-accent" />
            <h1 className="text-3xl font-semibold tracking-tight text-surface-foreground">
              Create Pool
            </h1>
          </div>
          <p className="text-sm text-surface-foreground/50 max-w-lg">
            Configure interest rate parameters and deploy a new lending pool.
          </p>
        </div>

        <form
          onSubmit={handleSubmit}
          noValidate
          className="flex flex-col gap-5 justify-center"
        >
          {/* Token Pair */}
          <Section
            title="Token Pair"
            icon={<Coins className="h-4 w-4" />}
          >
            <div className="grid grid-cols-2 gap-4">
              <Field id="lendToken" label="Lend Token">
                <TokenSelect
                  value={lendAddr}
                  onChange={setLendAddr}
                  options={TOKEN_OPTIONS}
                  placeholder="Select lend token"
                />
              </Field>

              <Field id="collateralToken" label="Collateral Token">
                <TokenSelect
                  value={collateralAddr}
                  onChange={setCollateralAddr}
                  options={TOKEN_OPTIONS}
                  placeholder="Select collateral token"
                />
              </Field>
            </div>
            {submitAttempted && !mintsReady && (
              <p className="text-[11px] text-[#d45677]">
                {lendAddr === collateralAddr
                  ? "Lend and collateral tokens must be different"
                  : "Select both tokens"}
              </p>
            )}
          </Section>

          {/* Price Feed */}
          <Section
            title="Price Feed"
            icon={<Radio className="h-4 w-4" />}
          >
            {!mintsReady ? (
              <p className="text-xs text-surface-foreground/35">
                Select both tokens above to configure the price feed.
              </p>
            ) : (
              <FeedPairPanel
                collateralMint={collateralMintPk!}
                lendMint={lendMintPk!}
                collateralSymbol={collateralSymbol}
                lendSymbol={lendSymbol}
              />
            )}
          </Section>

          {/* Fee Config */}
          <Section
            title="Interest Rate Model"
            icon={<Settings2 className="h-4 w-4" />}
          >
            <p className="text-xs text-surface-foreground/35 -mt-2">
              Control points define a continuous curve. Editing these updates the
              underlying segment parameters sent on-chain.
            </p>

            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <span className="w-12" />
                <span className="flex-1 text-xs font-semibold uppercase tracking-wider text-surface-foreground/45">
                  Utilization (%)
                </span>
                <span className="flex-1 text-xs font-semibold uppercase tracking-wider text-surface-foreground/45">
                  Rate (bps)
                </span>
              </div>

              {form.kinkPoints.map((point, i, arr) => (
                <div key={i} className="flex items-center gap-2">
                  <span className="w-12 text-right text-xs text-surface-foreground/45">
                    {i === 0 ? "Start" : i === arr.length - 1 ? "End" : "Kink"}
                  </span>
                  <div className="flex-1">
                    <input
                      type="number"
                      value={point.util}
                      min={0}
                      max={150}
                      step={1}
                      onChange={(e: ChangeEvent<HTMLInputElement>) =>
                        setKinkPoint(i, "util", e.target.value)
                      }
                      className={fieldClass(false)}
                    />
                  </div>
                  <div className="flex-1">
                    <input
                      type="number"
                      value={point.rate}
                      step="any"
                      onChange={(e: ChangeEvent<HTMLInputElement>) =>
                        setKinkPoint(i, "rate", e.target.value)
                      }
                      className={fieldClass(false)}
                    />
                  </div>
                </div>
              ))}

              {errors.kinkPoints && (
                <p className="text-xs text-destructive">{errors.kinkPoints}</p>
              )}
            </div>

            {pointsValid(form.kinkPoints) && (
              <IrmCurveChart points={form.kinkPoints.map(toIrmPoint)} />
            )}
          </Section>

          {/* LTV */}
          <Section
            title="Risk Parameters"
            icon={<ShieldCheck className="h-4 w-4" />}
          >
            <Field
              id="ltvPercent"
              label="Max LTV (%)"
              hint="Maximum loan-to-value ratio for borrowers (1–100)"
              error={submitAttempted ? errors.ltvPercent : undefined}
            >
              <NumberInput
                id="ltvPercent"
                value={form.ltvPercent}
                onChange={(v) => setForm((prev) => ({ ...prev, ltvPercent: v }))}
                min={1}
                max={100}
                step={1}
                placeholder="75"
                hasError={submitAttempted && !!errors.ltvPercent}
              />
            </Field>
          </Section>

          {/* Actions */}
          <div className="flex items-center justify-between pt-1">
            {!connected ? (
              <p className="text-xs text-destructive">Connect your wallet to deploy the pool</p>
            ) : submitAttempted && hasErrors ? (
              <p className="text-xs text-destructive">Fix the errors above before continuing</p>
            ) : (
              <span />
            )}

            {/* style-exception: glow shadow requires exact rgba for surface-accent color */}
            <button
              type="submit"
              disabled={isPending || !canSubmit}
              className={cn(
                "flex items-center gap-2 rounded-xl px-6 py-2.5 text-sm font-semibold transition-all duration-200",
                "bg-surface-accent text-surface shadow-[0_0_20px_rgba(198,152,229,0.30)]",
                "enabled:hover:brightness-110 enabled:hover:shadow-[0_0_28px_rgba(198,152,229,0.45)]",
                "enabled:active:scale-95",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              {isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Deploying…
                </>
              ) : (
                <>
                  <Plus className="h-4 w-4" />
                  Deploy Pool
                </>
              )}
            </button>
          </div >
        </form >
      </div >
    </div >
  );
}

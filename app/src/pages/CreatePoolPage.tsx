import { BackButton } from "@/components/common/BackButton";
import {
  useCreateLendingPool,
  type CreatePoolParams,
  type CreatePoolResult,
} from "@/hooks/program/useCreateLendingPool";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import {
  CheckCircle2,
  Copy,
  ExternalLink,
  Loader2,
  Plus,
  Settings2,
  ShieldCheck,
} from "lucide-react";
import { useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router";

// ─── helpers ──────────────────────────────────────────────────────────────────

function fieldClass(error: boolean) {
  return cn(
    "w-full rounded-xl border bg-[#c698e5]/[0.04] px-4 py-2.5 text-sm text-[#efe0f7]",
    "placeholder:text-[#efe0f7]/25 outline-none transition-colors duration-150",
    "focus:bg-[#c698e5]/[0.07] focus:border-[#c698e5]/50",
    error
      ? "border-[#d45677]/60 focus:border-[#d45677]"
      : "border-[#c698e5]/20 hover:border-[#c698e5]/35",
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
        className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45"
      >
        {label}
      </label>
      {children}
      {hint && !error && (
        <p className="text-[11px] text-[#efe0f7]/30">{hint}</p>
      )}
      {error && <p className="text-[11px] text-[#d45677]">{error}</p>}
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
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          {icon}
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">{title}</h2>
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
      <span className="text-[10px] font-semibold uppercase tracking-wider text-[#efe0f7]/35">
        {label}
      </span>
      <div className="flex items-center gap-2">
        <span className="flex-1 font-mono text-xs text-[#efe0f7]/80 break-all">
          {value}
        </span>
        <button
          type="button"
          onClick={copy}
          title="Copy to clipboard"
          className="shrink-0 rounded-lg p-1.5 text-[#efe0f7]/40 hover:bg-[#c698e5]/10 hover:text-[#c698e5] transition-colors cursor-pointer"
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}

// ─── page ─────────────────────────────────────────────────────────────────────

interface FormState {
  m1: string;
  c1: string;
  m2: string;
  c2: string;
  ltvPercent: string;
}

interface FormErrors {
  m1?: string;
  c1?: string;
  m2?: string;
  c2?: string;
  ltvPercent?: string;
}

const DEFAULT_FORM: FormState = {
  m1: "450",
  c1: "0",
  m2: "8000",
  c2: "-7173",
  ltvPercent: "97",
};

function validate(form: FormState): FormErrors {
  const errors: FormErrors = {};

  // Slopes (m1, m2) must be non-negative
  const slopeFields = ["m1", "m2"] as const;
  for (const k of slopeFields) {
    const n = Number(form[k]);
    if (form[k] === "" || isNaN(n) || n < 0) {
      errors[k] = "Must be a non-negative number";
    }
  }

  // Intercepts (c1, c2) can be negative (i64 on-chain)
  const interceptFields = ["c1", "c2"] as const;
  for (const k of interceptFields) {
    const n = Number(form[k]);
    if (form[k] === "" || isNaN(n)) {
      errors[k] = "Must be a valid number";
    }
  }

  const ltv = Number(form.ltvPercent);
  if (form.ltvPercent === "" || isNaN(ltv) || ltv <= 0 || ltv > 100) {
    errors.ltvPercent = "Must be between 1 and 100";
  }

  return errors;
}

export function CreatePoolPage() {
  const navigate = useNavigate();
  const { connected } = useWalletConnection();

  const [form, setForm] = useState<FormState>(DEFAULT_FORM);
  const [submitAttempted, setSubmitAttempted] = useState(false);
  const [result, setResult] = useState<CreatePoolResult | null>(null);

  const errors = validate(form);
  const hasErrors = Object.keys(errors).length > 0;

  const { mutateAsync, isPending } = useCreateLendingPool({
    onCreated: (r) => setResult(r),
  });

  function setField<K extends keyof FormState>(key: K, value: FormState[K]) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  function fieldError(key: keyof FormState): string | undefined {
    return submitAttempted ? errors[key] : undefined;
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitAttempted(true);
    if (hasErrors || !connected) return;

    const params: CreatePoolParams = {
      ltvPercent: Number(form.ltvPercent),
    };

    await mutateAsync(params);
  }

  // ── success screen ────────────────────────────────────────────────────────
  if (result) {
    return (
      <div className="w-full max-w-6xl mx-auto px-4 py-12">
        <BackButton to="/markets" label="Back to markets" />

        <div className="flex w-2/3 mx-auto flex-col justify-center mt-8">
          <div className="mb-8 flex items-center gap-3">
            <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-[#34d399]/15 text-[#34d399]">
              <CheckCircle2 className="h-5 w-5" />
            </span>
            <div>
              <h1 className="text-3xl font-semibold tracking-tight text-[#efe0f7]">
                Pool Deployed
              </h1>
              <p className="text-sm text-[#efe0f7]/50 mt-0.5">
                Save these addresses — they were generated automatically.
              </p>
            </div>
          </div>

          <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6 flex flex-col gap-4">
            <AddressRow
              label="Pool Address"
              value={result.poolAddress.toBase58()}
            />
            <div className="border-t border-[#c698e5]/10" />
            <AddressRow
              label="Collateral Mint"
              value={result.collateralMint.toBase58()}
            />
            <AddressRow label="Lend Mint" value={result.lendMint.toBase58()} />
          </div>

          <div className="flex gap-3 mt-6">
            <button
              onClick={() => navigate(`/pool/${result.poolAddress.toBase58()}`)}
              className={cn(
                "flex items-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold transition-all duration-200",
                "bg-[#c698e5] text-[#17081f] shadow-[0_0_20px_rgba(198,152,229,0.30)]",
                "hover:bg-[#d4aeee] hover:shadow-[0_0_28px_rgba(198,152,229,0.45)] active:scale-95 cursor-pointer",
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
              className="flex items-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold transition-all duration-200 border border-[#c698e5]/25 bg-[#c698e5]/8 text-[#c698e5] hover:border-[#c698e5]/50 hover:bg-[#c698e5]/15 active:scale-95 cursor-pointer"
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
            <Plus className="h-5 w-5 text-[#c698e5]" />
            <h1 className="text-3xl font-semibold tracking-tight text-[#efe0f7]">
              Create Pool
            </h1>
          </div>
          <p className="text-sm text-[#efe0f7]/50 max-w-lg">
            Configure interest rate parameters and deploy a new lending pool.
            Token mints are generated automatically and shown after deployment.
          </p>
        </div>

        <form
          onSubmit={handleSubmit}
          noValidate
          className="flex flex-col gap-5 justify-center"
        >
          {/* Fee Config */}
          <Section
            title="Interest Rate Model"
            icon={<Settings2 className="h-4 w-4" />}
          >
            <p className="text-[11px] text-[#efe0f7]/35 -mt-2">
              Two-slope model: rate&nbsp;=&nbsp;m·utilisation&nbsp;+&nbsp;c. The
              first slope applies below the kink, the second above it.
              Intercepts (c₁, c₂) can be negative for advanced curve shaping.
            </p>

            <div className="grid grid-cols-2 gap-4">
              <Field
                id="m1"
                label="m₁ — Low slope"
                hint="Multiplier for low utilisation"
                error={fieldError("m1")}
              >
                <NumberInput
                  id="m1"
                  value={form.m1}
                  onChange={(v) => setField("m1", v)}
                  min={0}
                  placeholder="0"
                  hasError={!!fieldError("m1")}
                />
              </Field>

              <Field
                id="c1"
                label="c₁ — Low intercept"
                hint="Base rate for low utilisation (bps, can be negative)"
                error={fieldError("c1")}
              >
                <NumberInput
                  id="c1"
                  value={form.c1}
                  onChange={(v) => setField("c1", v)}
                  placeholder="200"
                  hasError={!!fieldError("c1")}
                />
              </Field>

              <Field
                id="m2"
                label="m₂ — High slope"
                hint="Multiplier for high utilisation"
                error={fieldError("m2")}
              >
                <NumberInput
                  id="m2"
                  value={form.m2}
                  onChange={(v) => setField("m2", v)}
                  min={0}
                  placeholder="0"
                  hasError={!!fieldError("m2")}
                />
              </Field>

              <Field
                id="c2"
                label="c₂ — High intercept"
                hint="Base rate for high utilisation (bps, can be negative)"
                error={fieldError("c2")}
              >
                <NumberInput
                  id="c2"
                  value={form.c2}
                  onChange={(v) => setField("c2", v)}
                  placeholder="1000"
                  hasError={!!fieldError("c2")}
                />
              </Field>
            </div>
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
              error={fieldError("ltvPercent")}
            >
              <NumberInput
                id="ltvPercent"
                value={form.ltvPercent}
                onChange={(v) => setField("ltvPercent", v)}
                min={1}
                max={100}
                step={1}
                placeholder="75"
                hasError={!!fieldError("ltvPercent")}
              />
            </Field>
          </Section>

          {/* Actions */}
          <div className="flex items-center justify-between pt-1">
            {!connected && (
              <p className="text-xs text-[#d45677]">
                Connect your wallet to deploy the pool
              </p>
            )}
            {connected && submitAttempted && hasErrors && (
              <p className="text-xs text-[#d45677]">
                Fix the errors above before continuing
              </p>
            )}
            {connected && !(submitAttempted && hasErrors) && <span />}

            <button
              type="submit"
              disabled={isPending || !connected}
              className={cn(
                "flex items-center gap-2 rounded-xl px-6 py-2.5 text-sm font-semibold transition-all duration-200",
                "bg-[#c698e5] text-[#17081f] shadow-[0_0_20px_rgba(198,152,229,0.30)]",
                "enabled:hover:bg-[#d4aeee] enabled:hover:shadow-[0_0_28px_rgba(198,152,229,0.45)]",
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
          </div>
        </form>
      </div>
    </div>
  );
}

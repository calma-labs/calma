import { price_scale } from '@calma/wasm-lib'
import { ActionButton } from "@/components/common/ActionButton";
import { TokenSelect } from "@/components/ui/token-select";
import { usePullPythFeed } from "@/hooks/program/usePullPythFeed";
import { usePushPythFeed } from "@/hooks/program/usePushPythFeed";
import { getPriceFeedAccountForProgram } from "@pythnetwork/pyth-solana-receiver";
import { useFeedsByPair, type FeedByPair } from "@/hooks/program/useFeedsByPair";
import { useCreateFeed } from "@/hooks/program/useCreateFeed";
import { useSetFeedFromPyth } from "@/hooks/program/useSetFeedFromPyth";
import { useSetFeedManualValue } from "@/hooks/program/useSetFeedManualValue";
import { refreshFeedAccount } from "@/hooks/program/refreshFeedForDevnet";
import { usePythPrice } from "@/hooks/usePythPrice";
import { usePythFeeds } from "@/hooks/usePythFeeds";
import { type FeedRulesInput, noRules } from "@/config/feedRules";
import {
  pythQueryForToken,
  USDC_USD_FEED_ID,
  PLACEHOLDER_MINT,
  bytesToFeedIdHex,
} from "@/config/pythFeeds";
import { getTokenOptions } from "@/lib/tokenRegistry";
import { connection, feedPda, feedProgram } from "@/lib/program";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey, SendTransactionError, Transaction } from "@solana/web3.js";
import { MINTER_KEYPAIR } from "@/store/wallet.store";
import * as anchor from "@coral-xyz/anchor";
import {
  Activity,
  CheckCircle2,
  Loader2,
  Plus,
  Radio,
  Search,
  Send,
  Wallet,
} from "lucide-react";
import { useMemo, useState } from "react";

/** Tokens the Feed page can look up a Pyth feed for. */
const TOKEN_OPTIONS = getTokenOptions();

/** Turn a list of fetched Pyth feeds into `TokenSelect` options. */
function feedSelectOptions(
  feeds: { id: string; name: string; icon: string }[] | undefined,
) {
  return (feeds ?? []).map((f) => ({
    address: f.id,
    symbol: f.name,
    icon: f.icon,
  }));
}

function formatPrice(v: number): string {
  return v.toLocaleString("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: v < 1 ? 6 : 2,
  });
}

function relativeTime(unixSeconds: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000 - unixSeconds));
  if (secs < 2) return "just now";
  if (secs < 60) return `${secs}s ago`;
  return `${Math.floor(secs / 60)}m ago`;
}

function tryParsePubkey(s: string): PublicKey | null {
  if (!s) return null;
  try {
    return new PublicKey(s);
  } catch {
    return null;
  }
}

// ─── feed pusher tool (unified pull / push) ────────────────────────────────────

type PusherMode = "pull" | "push";

/**
 * Sponsored push feeds live at deterministic PDAs of the receiver program;
 * shard 0 hosts the primary sponsored set. Derive the `PriceUpdateV2` pubkey
 * from the Pyth feed_id — no runtime API call needed.
 */
function pushAccountForFeedId(feedId: string): PublicKey {
  const hex = feedId.startsWith("0x") ? feedId.slice(2) : feedId;
  return getPriceFeedAccountForProgram(0, Buffer.from(hex, "hex"));
}

function FeedPusherTool() {
  const { connected } = useWalletConnection();
  const [mode, setMode] = useState<PusherMode>("pull");

  // Token / Pyth feed selection — identical UX for both modes.
  const [tokenAddr, setTokenAddr] = useState(TOKEN_OPTIONS[0]?.address ?? "");
  const tokenSymbol =
    TOKEN_OPTIONS.find((o) => o.address === tokenAddr)?.symbol ?? "";
  const query = tokenSymbol ? pythQueryForToken(tokenSymbol) : null;

  const {
    data: feeds,
    isLoading: feedsLoading,
    error: feedsError,
  } = usePythFeeds(query);

  const [feedId, setFeedId] = useState("");
  const feed = feeds?.find((f) => f.id === feedId) ?? feeds?.[0] ?? null;

  const { data: price, isLoading: priceLoading, error: priceError } =
    usePythPrice(feed ? feed.id : null);

  // Both hooks are declared unconditionally; only the active mode's mutation
  // is invoked below.
  const pull = usePullPythFeed();
  const push = usePushPythFeed();
  const active = mode === "pull" ? pull : push;

  // Token mint seeds the feed PDA together with wSOL on the lend side, so
  // each token gets its own feed under the connected wallet.
  const collateralMint = useMemo(
    () => (tokenAddr ? tryParsePubkey(tokenAddr) : null),
    [tokenAddr],
  );
  const feedAddress = collateralMint
    ? feedPda(collateralMint, PLACEHOLDER_MINT).toBase58()
    : null;

  // Sponsored `PriceUpdateV2` account pubkeys — only meaningful in push mode
  // but cheap to derive so we can preview them under the feed picker.
  const collateralPushAccount = useMemo(
    () => (feed ? pushAccountForFeedId(feed.id) : null),
    [feed],
  );
  const lendPushAccount = useMemo(
    () => pushAccountForFeedId(USDC_USD_FEED_ID),
    [],
  );

  // Push coverage probe: does a sponsored PriceUpdateV2 actually exist at the
  // derived collateral address? The address is deterministic but Pyth only
  // maintains a crank for a curated subset — if this account is missing, push
  // won't work for this feed.
  const pushCoverage = useQuery({
    queryKey: [
      "push-coverage",
      collateralPushAccount?.toBase58() ?? null,
    ],
    enabled: mode === "push" && !!collateralPushAccount,
    queryFn: async () => {
      if (!collateralPushAccount) return null;
      const info = await connection.getAccountInfo(collateralPushAccount);
      return info !== null;
    },
  });

  // Existing feed source probe: if a feed already exists at `feedAddress`,
  // decode it and show its source so the user knows what's bound to this mint
  // pair (feeds are immutable per (wallet, coll_mint, lend_mint)).
  const existingFeed = useQuery<"manual" | "pyth" | "pythPush" | null>({
    // Prefixed with `feed-account` so the push/pull hooks' invalidation covers this.
    queryKey: ["feed-account", "source", feedAddress],
    enabled: !!feedAddress,
    queryFn: async () => {
      if (!feedAddress) return null;
      const info = await connection.getAccountInfo(new PublicKey(feedAddress));
      if (!info) return null;
      const decoded = feedProgram.coder.accounts.decode("feed", info.data) as {
        config: { source: Record<string, unknown> };
      };
      const src = decoded.config.source;
      return "pythPush" in src
        ? "pythPush"
        : "pyth" in src
          ? "pyth"
          : "manual" in src
            ? "manual"
            : null;
    },
  });

  async function handleSend() {
    if (!feed || !collateralMint) return;
    if (mode === "pull") {
      await pull.mutateAsync({
        feed,
        collateralMint,
        lendMint: PLACEHOLDER_MINT,
      });
    } else {
      if (!collateralPushAccount) return;
      await push.mutateAsync({
        collateralPushAccount,
        lendPushAccount,
        collateralMint,
        lendMint: PLACEHOLDER_MINT,
      });
    }
  }

  const sendLabel = active.isPending
    ? "Sending…"
    : !connected
      ? "Connect wallet"
      : mode === "pull"
        ? "Send pull oracle"
        : "Send push oracle";
  const sendDisabled =
    !connected ||
    active.isPending ||
    !feed ||
    (mode === "pull" && (priceLoading || !!priceError)) ||
    (mode === "push" && !collateralPushAccount);

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center justify-between gap-3 mb-5">
        <div className="flex items-center gap-2.5">
          <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
            <Radio className="h-4 w-4" />
          </span>
          <h2 className="text-sm font-semibold text-[#efe0f7]/80">
            Push Pyth Price
          </h2>
        </div>
        <ModeToggle mode={mode} onChange={setMode} />
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        {mode === "pull" ? (
          <>
            Select a token — its Pyth feed is looked up live from the Hermes
            catalog — watch the price, then post a signed price update to your
            wallet's <span className="font-mono">feed</span> account (priced
            against USDC/USD). Creates the feed on first use.
          </>
        ) : (
          <>
            The sponsored <span className="font-mono">PriceUpdateV2</span>{" "}
            account address is derived from the Pyth feed id via the receiver
            program's PDA seeds (shard&nbsp;0). No Hermes VAA is posted; the
            feed program pins directly to that account. Mainnet only.
          </>
        )}
      </p>

      <div className="flex flex-col gap-4">
        {/* token selector */}
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Token
          </label>
          <TokenSelect
            value={tokenAddr}
            onChange={setTokenAddr}
            options={TOKEN_OPTIONS}
            placeholder="Select a token"
          />
        </div>

        {/* resolved Pyth feed */}
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Pyth Feed
          </label>
          {feedsLoading ? (
            <div className="flex items-center gap-2 text-[#efe0f7]/40 text-sm px-1 py-2">
              <Loader2 className="h-4 w-4 animate-spin" /> Looking up feed…
            </div>
          ) : feedsError ? (
            <p className="text-sm text-[#d45677] px-1 py-2">
              {feedsError instanceof Error
                ? feedsError.message
                : "Feed lookup failed"}
            </p>
          ) : feeds && feeds.length > 0 ? (
            <TokenSelect
              value={feed?.id ?? ""}
              onChange={setFeedId}
              options={feedSelectOptions(feeds)}
              placeholder="Select a feed"
            />
          ) : (
            <p className="text-[11px] text-[#efe0f7]/30 px-1 py-2">
              No Pyth feed found for {tokenSymbol}.
            </p>
          )}
        </div>

        {/* pull-only: live price from Hermes */}
        {mode === "pull" && feed && (
          <div className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-4">
            <div className="flex items-center gap-1.5 mb-1.5">
              <Activity className="h-3 w-3 text-[#34d399]" />
              <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
                Live price · {feed.name}
              </p>
            </div>
            {priceLoading ? (
              <div className="flex items-center gap-2 text-[#efe0f7]/40 text-sm">
                <Loader2 className="h-4 w-4 animate-spin" /> Fetching from Hermes…
              </div>
            ) : priceError ? (
              <p className="text-sm text-[#d45677]">
                {priceError instanceof Error ? priceError.message : "Price unavailable"}
              </p>
            ) : price ? (
              <div className="flex items-baseline gap-3">
                <span className="font-mono text-2xl font-semibold text-[#efe0f7] tabular-nums">
                  {formatPrice(Number(price.price) / 1_000_000)}
                </span>
                <span className="text-[11px] text-[#efe0f7]/35">
                  ± {formatPrice(Number(price.confidence) / 1_000_000)} · {relativeTime(price.publishTime)}
                </span>
              </div>
            ) : null}
          </div>
        )}

        {/* push-only: derived sponsored PriceUpdateV2 pubkeys + coverage badge */}
        {mode === "push" && collateralPushAccount && (
          <div className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-4 flex flex-col gap-3">
            <div className="flex items-center gap-2">
              <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30 mb-1">
                Collateral price account
              </p>
              <PushCoverageBadge
                isLoading={pushCoverage.isLoading}
                exists={pushCoverage.data ?? null}
              />
            </div>
            <p className="font-mono text-xs text-[#efe0f7]/70 break-all">
              {collateralPushAccount.toBase58()}
            </p>
            <div>
              <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30 mb-1">
                Lend price account (USDC/USD)
              </p>
              <p className="font-mono text-xs text-[#efe0f7]/70 break-all">
                {lendPushAccount.toBase58()}
              </p>
            </div>
          </div>
        )}

        {/* target feed account + existing-source badge */}
        {feedAddress && (
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
                Target feed account
              </p>
              <FeedSourceBadge
                isLoading={existingFeed.isLoading}
                source={existingFeed.data ?? null}
              />
            </div>
            <p className="font-mono text-xs text-[#efe0f7]/60 break-all">
              {feedAddress}
            </p>
          </div>
        )}

        {active.error && (
          <p className="text-[11px] text-[#d45677]">
            {active.error instanceof Error ? active.error.message : "Unknown error"}
          </p>
        )}

        {active.data && (
          <div className="rounded-xl border border-[#34d399]/25 bg-[#34d399]/5 px-4 py-3">
            <div className="flex items-center gap-1.5 mb-1">
              <CheckCircle2 className="h-3.5 w-3.5 text-[#34d399]" />
              <p className="text-xs text-[#34d399]">
                {active.data.created ? "Feed created & priced" : "Price pushed"} ·{" "}
                {active.data.signatures.length} tx
              </p>
            </div>
            <div className="flex flex-col gap-0.5">
              {active.data.signatures.map((sig) => (
                <a
                  key={sig}
                  href={`https://solscan.io/tx/${sig}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-mono text-[10px] text-[#c698e5] hover:underline break-all"
                >
                  {sig}
                </a>
              ))}
            </div>
          </div>
        )}

        <div className="flex justify-end pt-1">
          <ActionButton
            variant="primary"
            onClick={handleSend}
            disabled={sendDisabled}
            label={sendLabel}
            icon={
              active.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin text-[#17081f]" />
              ) : !connected ? (
                <Wallet className="h-4 w-4 text-[#17081f]" />
              ) : (
                <Send className="h-4 w-4 text-[#17081f]" />
              )
            }
          />
        </div>
      </div>
    </div>
  );
}

function Pill({
  label,
  tone,
}: {
  label: string;
  tone: "ok" | "warn" | "muted";
}) {
  const toneClass =
    tone === "ok"
      ? "border-[#34d399]/40 bg-[#34d399]/10 text-[#34d399]"
      : tone === "warn"
        ? "border-[#e0b64d]/40 bg-[#e0b64d]/10 text-[#e0b64d]"
        : "border-[#c698e5]/25 bg-[#c698e5]/[0.06] text-[#efe0f7]/60";
  return (
    <span
      className={cn(
        "rounded-full border px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wider",
        toneClass,
      )}
    >
      {label}
    </span>
  );
}

function PushCoverageBadge({
  isLoading,
  exists,
}: {
  isLoading: boolean;
  exists: boolean | null;
}) {
  if (isLoading) return <Pill label="Checking…" tone="muted" />;
  if (exists === true) return <Pill label="Live sponsored" tone="ok" />;
  if (exists === false) return <Pill label="No sponsored crank" tone="warn" />;
  return null;
}

function FeedSourceBadge({
  isLoading,
  source,
}: {
  isLoading: boolean;
  source: "manual" | "pyth" | "pythPush" | null;
}) {
  if (isLoading) return <Pill label="Checking…" tone="muted" />;
  if (source === null) return <Pill label="Uncreated" tone="muted" />;
  if (source === "manual") return <Pill label="Manual" tone="muted" />;
  if (source === "pyth") return <Pill label="Pyth · pull" tone="ok" />;
  return <Pill label="Pyth · sponsored push" tone="ok" />;
}

function ModeToggle({
  mode,
  onChange,
}: {
  mode: PusherMode;
  onChange: (m: PusherMode) => void;
}) {
  const optionClass = (isActive: boolean) =>
    cn(
      "px-3 py-1 rounded-md text-[11px] font-semibold uppercase tracking-wider transition-colors",
      isActive
        ? "bg-[#c698e5]/20 text-[#efe0f7]"
        : "text-[#efe0f7]/40 hover:text-[#efe0f7]/70",
    );
  return (
    <div
      className="flex items-center gap-1 rounded-lg border border-[#c698e5]/15 bg-[#c698e5]/[0.03] p-0.5"
      role="tablist"
      aria-label="Feed source mode"
    >
      <button
        type="button"
        role="tab"
        aria-selected={mode === "pull"}
        className={optionClass(mode === "pull")}
        onClick={() => onChange("pull")}
      >
        Pull
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={mode === "push"}
        className={optionClass(mode === "push")}
        onClick={() => onChange("push")}
      >
        Push
      </button>
    </div>
  );
}

// ─── live pyth prices for a discovered feed ───────────────────────────────────

function FeedLivePrices({
  collateralFeedId,
  lendFeedId,
}: {
  collateralFeedId: number[];
  lendFeedId: number[];
}) {
  const collateralHex = useMemo(
    () => bytesToFeedIdHex(collateralFeedId),
    [collateralFeedId],
  );
  const lendHex = useMemo(
    () => bytesToFeedIdHex(lendFeedId),
    [lendFeedId],
  );

  const { data: collPrice, isLoading: cLoading } = usePythPrice(collateralHex);
  const { data: lendPr, isLoading: lLoading } = usePythPrice(lendHex);

  const loading = cLoading || lLoading;
  const liveRatio =
    collPrice && lendPr && lendPr.price > 0n
      ? Number(collPrice.price) / Number(lendPr.price)
      : null;

  return (
    <div className="mt-2 grid grid-cols-3 gap-3 border-t border-[#c698e5]/10 pt-2 text-[11px]">
      <div className="flex flex-col gap-0.5">
        <span className="flex items-center gap-1 text-[#efe0f7]/30">
          <Activity className="h-2.5 w-2.5 text-[#34d399]" />
          Live ratio
        </span>
        <span className="font-mono text-[#efe0f7]/80 tabular-nums">
          {loading ? "…" : liveRatio !== null ? formatPrice(liveRatio) : "—"}
        </span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="text-[#efe0f7]/30">Live coll px</span>
        <span className="font-mono text-[#efe0f7]/60 tabular-nums">
          {loading ? "…" : collPrice ? formatPrice(Number(collPrice.price) / 1_000_000) : "—"}
        </span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="text-[#efe0f7]/30">Live lend px</span>
        <span className="font-mono text-[#efe0f7]/60 tabular-nums">
          {loading ? "…" : lendPr ? formatPrice(Number(lendPr.price) / 1_000_000) : "—"}
        </span>
      </div>
    </div>
  );
}

// ─── feed finder (by mint pair) ─────────────────────────────────────────────────

/** Prices on a feed are stored scaled by 1e6 (see `PRICE_SCALE`). */
const PRICE_SCALE = price_scale();

function shorten(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

/** collateral / lend, formatted as a plain ratio. */
function formatRatio(collateral: bigint, lend: bigint): string {
  if (lend === 0n) return "—";
  const scaled = (collateral * PRICE_SCALE) / lend; // 6 fractional digits
  return (Number(scaled) / Number(PRICE_SCALE)).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  });
}

function formatScaled(v: bigint): string {
  return (Number(v) / Number(PRICE_SCALE)).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  });
}

/** Prices are stored scaled by PRICE_SCALE (1e6) on-chain. */
const PRICE_SCALE_N = price_scale();

interface RulesInputStrings {
  maxConfBps: string;
  maxDeviationBpsPerHour: string;
  emaDivergenceBps: string;
  minPriceUsd: string;
  maxPriceUsd: string;
  maxAgeSecs: string;
}

/** Parse a form field: empty → 0; otherwise expect a non-negative number. */
function parseOptional(field: string): number | null {
  const s = field.trim();
  if (s === "") return 0;
  const n = Number(s);
  if (!Number.isFinite(n) || n < 0) return null;
  return n;
}

/** Convert a USD-denominated form field into an on-chain PRICE_SCALE u64 (as BN). */
function usdToScaledBn(field: string): anchor.BN | null {
  const n = parseOptional(field);
  if (n === null) return null;
  if (n === 0) return new anchor.BN(0);
  // Multiply in BigInt to avoid float imprecision on the boundary.
  const scaled = BigInt(Math.round(n * Number(PRICE_SCALE_N)));
  return new anchor.BN(scaled.toString());
}

/**
 * Human-readable validation for the rules form. Returns a message when a field
 * is malformed; `null` when all values are acceptable (0 or positive).
 */
function validateRulesInputs(v: RulesInputStrings): string | null {
  const bpsFields: Array<[string, string]> = [
    ["Max confidence", v.maxConfBps],
    ["Max deviation", v.maxDeviationBpsPerHour],
    ["EMA divergence", v.emaDivergenceBps],
  ];
  for (const [label, raw] of bpsFields) {
    const n = parseOptional(raw);
    if (n === null) return `${label} must be a non-negative number.`;
    if (!Number.isInteger(n)) return `${label} must be an integer bps value.`;
    if (n > 65_535) return `${label} exceeds the u16 limit (65535).`;
  }
  const min = parseOptional(v.minPriceUsd);
  const max = parseOptional(v.maxPriceUsd);
  if (min === null) return "Min price must be a non-negative number.";
  if (max === null) return "Max price must be a non-negative number.";
  if (min > 0 && max > 0 && min > max) {
    return "Min price cannot exceed max price.";
  }
  const age = Number(v.maxAgeSecs.trim());
  if (!Number.isFinite(age) || age <= 0) return "Max age must be a positive number (seconds).";
  return null;
}

/**
 * Build a `FeedRulesInput` from the current form values. Callers must pass
 * fields that have already cleared `validateRulesInputs`, so parsing is
 * infallible here.
 */
function buildRulesInput(v: RulesInputStrings): FeedRulesInput {
  const base = noRules();
  return {
    ...base,
    maxConfBps: parseOptional(v.maxConfBps) ?? 0,
    maxDeviationBpsPerHour: parseOptional(v.maxDeviationBpsPerHour) ?? 0,
    emaDivergenceBps: parseOptional(v.emaDivergenceBps) ?? 0,
    minPrice: usdToScaledBn(v.minPriceUsd) ?? new anchor.BN(0),
    maxPrice: usdToScaledBn(v.maxPriceUsd) ?? new anchor.BN(0),
    maxAgeMs: Math.round(Number(v.maxAgeSecs.trim()) * 1000),
  };
}

interface RuleInputProps {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}

function RuleInput({ label, hint, value, onChange, placeholder }: RuleInputProps) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-[10px] uppercase tracking-wider text-[#efe0f7]/40">
        {label}
      </label>
      <input
        type="number"
        inputMode="decimal"
        min="0"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={cn(
          "w-full rounded-lg border border-[#c698e5]/20 bg-[#c698e5]/[0.04]",
          "px-3 py-2 text-sm font-mono text-[#efe0f7] tabular-nums outline-none",
          "placeholder:text-[#efe0f7]/25 focus:border-[#c698e5]/40",
        )}
      />
      <p className="text-[10px] text-[#efe0f7]/30">{hint}</p>
    </div>
  );
}

interface CreateFeedCardProps {
  collateralMint: PublicKey;
  lendMint: PublicKey;
  collateralSymbol: string;
  lendSymbol: string;
}

function CreateFeedCard({
  collateralMint,
  lendMint,
  collateralSymbol,
  lendSymbol,
}: CreateFeedCardProps) {
  const { connected } = useWalletConnection();

  // Each side's Pyth feed is looked up 1:1 from its token symbol via Hermes'
  // catalog. When a lookup returns several USD feeds the user picks the exact
  // one; otherwise the sole match is auto-selected.
  const { data: collateralFeeds } = usePythFeeds(
    collateralSymbol ? pythQueryForToken(collateralSymbol) : null,
  );
  const { data: lendFeeds } = usePythFeeds(
    lendSymbol ? pythQueryForToken(lendSymbol) : null,
  );

  // Explicit user choice per side, falling back to the first fetched match.
  const [collateralChoice, setCollateralChoice] = useState("");
  const [lendChoice, setLendChoice] = useState("");

  // Optional Pyth-side validation rules baked into the feed at create time.
  // Fields are strings so the inputs stay controlled; empty / "0" → disabled,
  // matching the on-chain `FeedRules::default()` sentinel.
  const [maxConfBps, setMaxConfBps] = useState("");
  const [maxDeviationBpsPerHour, setMaxDeviationBpsPerHour] = useState("");
  const [emaDivergenceBps, setEmaDivergenceBps] = useState("");
  const [minPriceUsd, setMinPriceUsd] = useState("");
  const [maxPriceUsd, setMaxPriceUsd] = useState("");
  const [maxAgeSecs, setMaxAgeSecs] = useState("10");

  const collateralFeed =
    collateralFeeds?.find((f) => f.id === collateralChoice) ??
    collateralFeeds?.[0] ??
    null;
  const lendFeed =
    lendFeeds?.find((f) => f.id === lendChoice) ?? lendFeeds?.[0] ?? null;

  const collateralFeedId = collateralFeed?.id ?? "";
  const lendFeedId = lendFeed?.id ?? "";

  const { data: collateralPrice } = usePythPrice(collateralFeedId || null);
  const { data: lendPrice } = usePythPrice(lendFeedId || null);

  const { mutateAsync, isPending, error, data: signature } = useCreateFeed();

  const pairPrice =
    collateralPrice && lendPrice && lendPrice.price > 0n
      ? Number(collateralPrice.price) / Number(lendPrice.price)
      : null;

  const feedsResolved = !!collateralFeedId && !!lendFeedId;

  const rulesError = validateRulesInputs({
    maxConfBps,
    maxDeviationBpsPerHour,
    emaDivergenceBps,
    minPriceUsd,
    maxPriceUsd,
    maxAgeSecs,
  });

  async function handleCreate() {
    if (!feedsResolved || rulesError) return;
    const rules = buildRulesInput({
      maxConfBps,
      maxDeviationBpsPerHour,
      emaDivergenceBps,
      minPriceUsd,
      maxPriceUsd,
      maxAgeSecs,
    });
    await mutateAsync({
      collateralMint,
      lendMint,
      collateralFeedId,
      lendFeedId,
      rules,
    });
  }

  return (
    <div className="rounded-xl border border-[#c698e5]/20 bg-[#c698e5]/[0.04] p-5">
      <div className="flex items-center gap-2 mb-4">
        <Plus className="h-4 w-4 text-[#c698e5]" />
        <h3 className="text-sm font-semibold text-[#efe0f7]/80">
          Create a feed for this pair
        </h3>
      </div>

      <div className="flex flex-col gap-4">
        {/* type */}
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Type
          </label>
          <select
            value="pyth"
            disabled
            className={cn(
              "w-full appearance-none rounded-xl border border-[#c698e5]/20 bg-[#c698e5]/[0.04]",
              "px-4 py-2.5 text-sm text-[#efe0f7] outline-none cursor-not-allowed",
            )}
          >
            <option value="pyth">Pyth</option>
          </select>
        </div>

        {/* pyth feed bindings — resolved 1:1 from each side's token */}
        <div className="grid grid-cols-2 gap-3">
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
              Collateral feed · {collateralSymbol || "—"}
            </label>
            {collateralFeeds && collateralFeeds.length > 0 ? (
              <TokenSelect
                value={collateralFeedId}
                onChange={setCollateralChoice}
                options={feedSelectOptions(collateralFeeds)}
                placeholder="Collateral Pyth feed"
              />
            ) : (
              <p className="text-[11px] text-[#efe0f7]/30 px-1 py-2.5">
                {collateralFeeds ? "No Pyth feed found" : "Looking up feed…"}
              </p>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
              Lend feed · {lendSymbol || "—"}
            </label>
            {lendFeeds && lendFeeds.length > 0 ? (
              <TokenSelect
                value={lendFeedId}
                onChange={setLendChoice}
                options={feedSelectOptions(lendFeeds)}
                placeholder="Lend Pyth feed"
              />
            ) : (
              <p className="text-[11px] text-[#efe0f7]/30 px-1 py-2.5">
                {lendFeeds ? "No Pyth feed found" : "Looking up feed…"}
              </p>
            )}
          </div>
        </div>

        {/* live pair price */}
        <div className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-3">
          <div className="flex items-center gap-1.5 mb-1">
            <Activity className="h-3 w-3 text-[#34d399]" />
            <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
              Current price · collateral / lend
            </p>
          </div>
          {pairPrice !== null ? (
            <div className="flex items-baseline gap-2">
              <span className="font-mono text-xl font-semibold text-[#efe0f7] tabular-nums">
                {formatPrice(pairPrice)}
              </span>
              {collateralPrice && (
                <span className="text-[11px] text-[#efe0f7]/35">
                  {relativeTime(collateralPrice.publishTime)}
                </span>
              )}
            </div>
          ) : (
            <div className="flex items-center gap-2 text-[#efe0f7]/40 text-sm">
              <Loader2 className="h-4 w-4 animate-spin" /> Fetching from Hermes…
            </div>
          )}

          {/* individual token prices */}
          <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[#c698e5]/10 pt-3">
            <div className="flex flex-col gap-0.5">
              <span className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
                {collateralSymbol || "Collateral"}
              </span>
              <span className="font-mono text-sm text-[#efe0f7]/80 tabular-nums">
                {collateralPrice ? formatPrice(Number(collateralPrice.price) / 1_000_000) : "—"}
              </span>
            </div>
            <div className="flex flex-col gap-0.5">
              <span className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
                {lendSymbol || "Lend"}
              </span>
              <span className="font-mono text-sm text-[#efe0f7]/80 tabular-nums">
                {lendPrice ? formatPrice(Number(lendPrice.price) / 1_000_000) : "—"}
              </span>
            </div>
          </div>
        </div>

        {/* optional validation rules */}
        <details className="group rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-3 open:pb-4">
          <summary className="flex cursor-pointer items-center justify-between gap-2 text-[11px] uppercase tracking-wider text-[#efe0f7]/45 list-none">
            <span>Optional validation rules · advanced</span>
            <span className="text-[#efe0f7]/30 group-open:hidden">Show</span>
            <span className="text-[#efe0f7]/30 hidden group-open:inline">Hide</span>
          </summary>
          <p className="mt-2 text-[10px] text-[#efe0f7]/35">
            All fields are optional. Leave empty (or 0) to disable a rule. Rules
            are fixed at create time and enforced when this feed is refreshed via{" "}
            <span className="font-mono">set_from_pyth</span>.
          </p>
          <div className="mt-3 grid grid-cols-2 gap-3">
            <RuleInput
              label="Max confidence (bps)"
              hint="Reject if conf/price exceeds this. 100 = 1%."
              value={maxConfBps}
              onChange={setMaxConfBps}
              placeholder="e.g. 100"
            />
            <RuleInput
              label="Max deviation (bps/hour)"
              hint="Time-scaled circuit breaker. 500 = 5% per hour."
              value={maxDeviationBpsPerHour}
              onChange={setMaxDeviationBpsPerHour}
              placeholder="e.g. 500"
            />
            <RuleInput
              label="EMA divergence (bps)"
              hint="Reject if spot diverges from Pyth EMA by more than this."
              value={emaDivergenceBps}
              onChange={setEmaDivergenceBps}
              placeholder="e.g. 300"
            />
            <RuleInput
              label="Min price (USD)"
              hint="Absolute floor on normalized price."
              value={minPriceUsd}
              onChange={setMinPriceUsd}
              placeholder="e.g. 0.5"
            />
            <RuleInput
              label="Max price (USD)"
              hint="Absolute ceiling on normalized price."
              value={maxPriceUsd}
              onChange={setMaxPriceUsd}
              placeholder="e.g. 1000000"
            />
            <RuleInput
              label="Max age (seconds)"
              hint="Reject Pyth prices older than this. Required (> 0)."
              value={maxAgeSecs}
              onChange={setMaxAgeSecs}
              placeholder="e.g. 10"
            />
          </div>
          {rulesError && (
            <p className="mt-3 text-[11px] text-[#d45677]">{rulesError}</p>
          )}
        </details>

        {error && (
          <p className="text-[11px] text-[#d45677]">
            {error instanceof Error ? error.message : "Unknown error"}
          </p>
        )}

        {signature && (
          <div className="rounded-xl border border-[#34d399]/25 bg-[#34d399]/5 px-4 py-3">
            <div className="flex items-center gap-1.5 mb-1">
              <CheckCircle2 className="h-3.5 w-3.5 text-[#34d399]" />
              <p className="text-xs text-[#34d399]">Feed created</p>
            </div>
            <a
              href={`https://solscan.io/tx/${signature}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-[10px] text-[#c698e5] hover:underline break-all"
            >
              {signature}
            </a>
          </div>
        )}

        <div className="flex justify-end pt-1">
          <ActionButton
            variant="primary"
            onClick={handleCreate}
            disabled={!connected || isPending || !feedsResolved || !!rulesError}
            label={isPending ? "Creating…" : !connected ? "Connect wallet" : "Create feed"}
            icon={
              isPending ? (
                <Loader2 className="h-4 w-4 animate-spin text-[#17081f]" />
              ) : !connected ? (
                <Wallet className="h-4 w-4 text-[#17081f]" />
              ) : (
                <Plus className="h-4 w-4 text-[#17081f]" />
              )
            }
          />
        </div>
      </div>
    </div>
  );
}

interface FeedUpdaterActionsProps {
  feed: FeedByPair;
  collateralMint: PublicKey;
  lendMint: PublicKey;
}

function FeedUpdaterActions({
  feed,
  collateralMint,
  lendMint,
}: FeedUpdaterActionsProps) {
  if (feed.source === "Pyth") {
    return (
      <PythFeedUpdater
        feed={feed}
        collateralMint={collateralMint}
        lendMint={lendMint}
      />
    );
  }
  if (feed.source === "Manual") {
    return (
      <ManualFeedUpdater
        feed={feed}
        collateralMint={collateralMint}
        lendMint={lendMint}
      />
    );
  }
  return null;
}

function PythFeedUpdater({
  feed,
  collateralMint,
  lendMint,
}: FeedUpdaterActionsProps) {
  const { connected } = useWalletConnection();
  const {
    collateralAccount,
    lendAccount,
    busy,
    signatures,
    error,
    loadSide,
    commit,
    reset,
  } = useSetFeedFromPyth();
  const [bgPending, setBgPending] = useState(false);
  const [bgError, setBgError] = useState<string | null>(null);
  const [bgSignatures, setBgSignatures] = useState<string[]>([]);

  const refreshCtx = {
    feed: feed.publicKey,
    collateralFeedIdBytes: feed.collateralFeedId,
    lendFeedIdBytes: feed.lendFeedId,
    collateralMint,
    lendMint,
  };

  const anythingLoaded =
    collateralAccount !== null ||
    lendAccount !== null ||
    signatures.commit.length > 0;

  async function handleBackground() {
    setBgPending(true);
    setBgError(null);
    setBgSignatures([]);
    try {
      const sigs = await refreshFeedAccount(
        connection,
        feed.publicKey,
        feed.collateralFeedId,
        feed.lendFeedId,
      );
      setBgSignatures(sigs);
    } catch (e) {
      if (e instanceof SendTransactionError) {
        const anchor = e.logs?.find((l: string) => l.includes("Error Code:") || l.includes("AnchorError"));
        setBgError(anchor ?? e.message);
      } else {
        setBgError(e instanceof Error ? e.message : "Background refresh failed");
      }
    } finally {
      setBgPending(false);
    }
  }

  return (
    <div className="mt-3 flex flex-col gap-2 border-t border-[#c698e5]/10 pt-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
          Refresh from Pyth Hermes
        </p>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleBackground}
            disabled={bgPending || busy !== null}
            className={cn(
              "flex items-center gap-1.5 rounded-lg border border-[#efe0f7]/15 bg-[#efe0f7]/[0.04]",
              "px-2.5 py-1 text-[10px] font-semibold text-[#efe0f7]/50",
              "hover:bg-[#efe0f7]/[0.08] disabled:opacity-40 disabled:cursor-not-allowed",
            )}
          >
            {bgPending ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Send className="h-3 w-3" />
            )}
            {bgPending ? "Sending…" : "Run as MINTER"}
          </button>
          {anythingLoaded && (
            <button
              type="button"
              onClick={reset}
              className={cn(
                "text-[10px] uppercase tracking-wider text-[#efe0f7]/40",
                "hover:text-[#efe0f7]/70",
              )}
            >
              Reset
            </button>
          )}
        </div>
      </div>
      {bgError && <p className="text-[10px] text-[#d45677]">{bgError}</p>}
      {bgSignatures.map((sig) => (
        <a
          key={sig}
          href={`https://solscan.io/tx/${sig}?cluster=devnet`}
          target="_blank"
          rel="noopener noreferrer"
          className="font-mono text-[10px] text-[#34d399]/80 hover:underline break-all"
        >
          {sig}
        </a>
      ))}

      <div className="flex flex-col gap-2">
        <FeedRefreshRow
          label="Load collateral price"
          side="collateral"
          account={collateralAccount}
          signatures={signatures.collateral}
          busy={busy}
          onClick={() => loadSide("collateral", refreshCtx)}
          disabled={!connected || busy !== null || collateralAccount !== null}
        />
        <FeedRefreshRow
          label="Load loaned price"
          side="lend"
          account={lendAccount}
          signatures={signatures.lend}
          busy={busy}
          onClick={() => loadSide("lend", refreshCtx)}
          disabled={!connected || busy !== null || lendAccount !== null}
        />
        <FeedRefreshRow
          label="Commit set_from_pyth"
          side="commit"
          account={null}
          signatures={signatures.commit}
          busy={busy}
          onClick={() => commit(refreshCtx)}
          disabled={
            !connected ||
            busy !== null ||
            collateralAccount === null ||
            lendAccount === null ||
            signatures.commit.length > 0
          }
        />
      </div>

      {error && <p className="text-[10px] text-[#d45677]">{error}</p>}
    </div>
  );
}

interface FeedRefreshRowProps {
  label: string;
  side: "collateral" | "lend" | "commit";
  account: PublicKey | null;
  signatures: string[];
  busy: "collateral" | "lend" | "commit" | null;
  onClick: () => void;
  disabled: boolean;
}

function FeedRefreshRow({
  label,
  side,
  account,
  signatures,
  busy,
  onClick,
  disabled,
}: FeedRefreshRowProps) {
  const isBusy = busy === side;
  const done = signatures.length > 0;
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex items-center justify-between gap-2">
        <span
          className={cn(
            "text-[11px]",
            done
              ? "text-[#34d399]"
              : isBusy
              ? "text-[#efe0f7]/80"
              : "text-[#efe0f7]/50",
          )}
        >
          {label}
          {signatures.length > 1 && (
            <span className="ml-1 text-[#efe0f7]/35">
              ({signatures.length} tx)
            </span>
          )}
        </span>
        <button
          type="button"
          onClick={onClick}
          disabled={disabled}
          className={cn(
            "flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[10px] font-semibold",
            done
              ? "border-[#34d399]/25 bg-[#34d399]/10 text-[#34d399]"
              : "border-[#c698e5]/25 bg-[#c698e5]/[0.06] text-[#c698e5] hover:bg-[#c698e5]/[0.12]",
            "disabled:opacity-40 disabled:cursor-not-allowed",
          )}
        >
          {isBusy ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : done ? (
            <CheckCircle2 className="h-3 w-3" />
          ) : (
            <Send className="h-3 w-3" />
          )}
          {done ? "Done" : isBusy ? "Sending…" : "Run"}
        </button>
      </div>
      {account && (
        <p className="font-mono text-[10px] text-[#efe0f7]/40 break-all">
          update acct {account.toBase58()}
        </p>
      )}
      {signatures.length > 0 && (
        <div className="flex flex-col gap-0.5">
          {signatures.map((sig) => (
            <a
              key={sig}
              href={`https://solscan.io/tx/${sig}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-[10px] text-[#34d399]/80 hover:underline break-all"
            >
              {sig}
            </a>
          ))}
        </div>
      )}
    </div>
  );
}

function ManualFeedUpdater({
  feed,
  collateralMint,
  lendMint,
}: FeedUpdaterActionsProps) {
  const { connected, wallet } = useWalletConnection();
  const walletPubkey = wallet
    ? new PublicKey(wallet.account.publicKey)
    : null;
  const isAuthority = !!walletPubkey && walletPubkey.equals(feed.authority);

  const [collateralUsd, setCollateralUsd] = useState("");
  const [lendUsd, setLendUsd] = useState("");
  const { mutateAsync, isPending, error, data: signature } =
    useSetFeedManualValue();
  const [bgPending, setBgPending] = useState(false);
  const [bgError, setBgError] = useState<string | null>(null);
  const [bgSignature, setBgSignature] = useState<string | null>(null);

  const parsedCollateral = usdToScaledBn(collateralUsd);
  const parsedLend = usdToScaledBn(lendUsd);
  const canSubmit =
    connected &&
    isAuthority &&
    parsedCollateral !== null &&
    parsedLend !== null &&
    !parsedCollateral.isZero() &&
    !parsedLend.isZero();
  const canBg =
    parsedCollateral !== null &&
    parsedLend !== null &&
    !parsedCollateral.isZero() &&
    !parsedLend.isZero();

  async function handleSubmit() {
    if (!canSubmit) return;
    await mutateAsync({
      feed: feed.publicKey,
      collateralPrice: parsedCollateral!,
      lendPrice: parsedLend!,
      collateralMint,
      lendMint,
    });
  }

  async function handleBackground() {
    if (!canBg) return;
    setBgPending(true);
    setBgError(null);
    setBgSignature(null);
    try {
      const ix = await feedProgram.methods
        .setValue(parsedCollateral!, parsedLend!)
        .accountsPartial({ feed: feed.publicKey, authority: feed.authority })
        .instruction();
      const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
      const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: MINTER_KEYPAIR.publicKey }).add(ix);
      tx.sign(MINTER_KEYPAIR);
      const sig = await connection.sendRawTransaction(tx.serialize());
      await connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
      setBgSignature(sig);
    } catch (e) {
      if (e instanceof SendTransactionError) {
        const anchor = e.logs?.find((l: string) => l.includes("Error Code:") || l.includes("AnchorError"));
        setBgError(anchor ?? e.message);
      } else {
        setBgError(e instanceof Error ? e.message : "Background update failed");
      }
    } finally {
      setBgPending(false);
    }
  }

  return (
    <div className="mt-3 flex flex-col gap-2 border-t border-[#c698e5]/10 pt-3">
      <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
        Push manual price
      </p>
      {!isAuthority ? (
        <p className="text-[10px] text-[#efe0f7]/35">
          Only the feed's authority ({shorten(feed.authority.toBase58())}) can
          update a Manual feed. Connect that wallet to push a price.
        </p>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-2">
            <input
              type="number"
              inputMode="decimal"
              min="0"
              value={collateralUsd}
              onChange={(e) => setCollateralUsd(e.target.value)}
              placeholder="Collateral price (USD)"
              className={cn(
                "w-full rounded-lg border border-[#c698e5]/20 bg-[#c698e5]/[0.04]",
                "px-3 py-1.5 text-xs font-mono text-[#efe0f7] tabular-nums outline-none",
                "placeholder:text-[#efe0f7]/25 focus:border-[#c698e5]/40",
              )}
            />
            <input
              type="number"
              inputMode="decimal"
              min="0"
              value={lendUsd}
              onChange={(e) => setLendUsd(e.target.value)}
              placeholder="Lend price (USD)"
              className={cn(
                "w-full rounded-lg border border-[#c698e5]/20 bg-[#c698e5]/[0.04]",
                "px-3 py-1.5 text-xs font-mono text-[#efe0f7] tabular-nums outline-none",
                "placeholder:text-[#efe0f7]/25 focus:border-[#c698e5]/40",
              )}
            />
          </div>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={handleBackground}
              disabled={!canBg || bgPending || isPending}
              className={cn(
                "flex items-center gap-1.5 rounded-lg border border-[#efe0f7]/15 bg-[#efe0f7]/[0.04]",
                "px-3 py-1.5 text-[11px] font-semibold text-[#efe0f7]/50",
                "hover:bg-[#efe0f7]/[0.08] disabled:opacity-40 disabled:cursor-not-allowed",
              )}
            >
              {bgPending ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Send className="h-3 w-3" />
              )}
              {bgPending ? "Sending…" : "Run as MINTER"}
            </button>
            <button
              type="button"
              onClick={handleSubmit}
              disabled={!canSubmit || isPending}
              className={cn(
                "flex items-center gap-1.5 rounded-lg border border-[#c698e5]/25 bg-[#c698e5]/[0.06]",
                "px-3 py-1.5 text-[11px] font-semibold text-[#c698e5]",
                "hover:bg-[#c698e5]/[0.12] disabled:opacity-40 disabled:cursor-not-allowed",
              )}
            >
              {isPending ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Send className="h-3 w-3" />
              )}
              {isPending ? "Updating…" : "Update"}
            </button>
          </div>
          {bgError && (
            <p className="text-[10px] text-[#d45677]">{bgError}</p>
          )}
          {bgSignature && (
            <a
              href={`https://solscan.io/tx/${bgSignature}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-[10px] text-[#34d399] hover:underline break-all"
            >
              {bgSignature}
            </a>
          )}
          {error && (
            <p className="text-[10px] text-[#d45677]">
              {error instanceof Error ? error.message : "Update failed"}
            </p>
          )}
          {signature && (
            <a
              href={`https://solscan.io/tx/${signature}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-[10px] text-[#34d399] hover:underline break-all"
            >
              {signature}
            </a>
          )}
        </>
      )}
    </div>
  );
}

function FeedFinderTool() {
  const [collateralAddr, setCollateralAddr] = useState<string>("");
  const [lendAddr, setLendAddr] = useState<string>("");

  const collateralMint = useMemo(
    () => (collateralAddr ? new PublicKey(collateralAddr) : null),
    [collateralAddr],
  );
  const lendMint = useMemo(
    () => (lendAddr ? new PublicKey(lendAddr) : null),
    [lendAddr],
  );

  const { data: feeds, isLoading, error, isFetched } = useFeedsByPair(
    collateralMint,
    lendMint,
  );

  const collateralSymbol =
    TOKEN_OPTIONS.find((o) => o.address === collateralAddr)?.symbol ?? "";
  const lendSymbol =
    TOKEN_OPTIONS.find((o) => o.address === lendAddr)?.symbol ?? "";

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          <Search className="h-4 w-4" />
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">
          Find Feeds by Pair
        </h2>
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        Pick a collateral and lend token. The page queries every on-chain{" "}
        <span className="font-mono">feed</span> account declaring for that mint
        pair using an RPC <span className="font-mono">memcmp</span> filter.
      </p>

      <div className="flex flex-col gap-4">
        {/* pair selectors */}
        <div className="grid grid-cols-2 gap-3">
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
              Collateral
            </label>
            <TokenSelect
              value={collateralAddr}
              onChange={setCollateralAddr}
              options={TOKEN_OPTIONS}
              placeholder="Collateral token"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
              Lend
            </label>
            <TokenSelect
              value={lendAddr}
              onChange={setLendAddr}
              options={TOKEN_OPTIONS}
              placeholder="Lend token"
            />
          </div>
        </div>

        {/* results */}
        {!collateralMint || !lendMint ? (
          <p className="text-[11px] text-[#efe0f7]/30 py-2">
            Select both tokens to search.
          </p>
        ) : isLoading ? (
          <div className="flex items-center gap-2 text-[#efe0f7]/40 text-sm py-2">
            <Loader2 className="h-4 w-4 animate-spin" /> Querying feeds…
          </div>
        ) : error ? (
          <p className="text-sm text-[#d45677]">
            {error instanceof Error ? error.message : "Failed to load feeds"}
          </p>
        ) : feeds && feeds.length > 0 ? (
          <div className="flex flex-col gap-2">
            <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
              {feeds.length} feed{feeds.length === 1 ? "" : "s"} found
            </p>
            {feeds.map((f) => (
              <div
                key={f.publicKey.toBase58()}
                className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-3"
              >
                <div className="flex items-center justify-between gap-2 mb-2">
                  <span className="font-mono text-xs text-[#efe0f7]/70 break-all">
                    {shorten(f.publicKey.toBase58())}
                  </span>
                  <span
                    className={cn(
                      "rounded-md px-2 py-0.5 text-[10px] font-semibold",
                      f.source === "Pyth"
                        ? "bg-[#c698e5]/15 text-[#c698e5]"
                        : "bg-[#efe0f7]/10 text-[#efe0f7]/50",
                    )}
                  >
                    {f.source}
                  </span>
                </div>
                <div className="grid grid-cols-3 gap-3 text-[11px]">
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[#efe0f7]/30">Program ratio</span>
                    <span className="font-mono text-[#efe0f7]/80 tabular-nums">
                      {formatRatio(f.collateralPrice, f.lendPrice)}
                    </span>
                  </div>
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[#efe0f7]/30">Program coll px</span>
                    <span className="font-mono text-[#efe0f7]/60 tabular-nums">
                      {formatScaled(f.collateralPrice)}
                    </span>
                  </div>
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[#efe0f7]/30">Program lend px</span>
                    <span className="font-mono text-[#efe0f7]/60 tabular-nums">
                      {formatScaled(f.lendPrice)}
                    </span>
                  </div>
                </div>
                {(f.source === "Pyth" || f.source === "PythPush") && (
                  <FeedLivePrices
                    collateralFeedId={f.collateralFeedId}
                    lendFeedId={f.lendFeedId}
                  />
                )}
                <div className="mt-2 flex items-center justify-between text-[10px] text-[#efe0f7]/30">
                  <span className="font-mono">auth {shorten(f.authority.toBase58())}</span>
                  <span>
                    {f.lastUpdatedTs > 0
                      ? `updated ${relativeTime(f.lastUpdatedTs)}`
                      : "never updated"}
                  </span>
                </div>
                <FeedUpdaterActions
                  feed={f}
                  collateralMint={collateralMint!}
                  lendMint={lendMint!}
                />
              </div>
            ))}
            <details className="group">
              <summary className="flex cursor-pointer list-none items-center gap-2 py-1 text-[10px] uppercase tracking-wider text-[#efe0f7]/35 hover:text-[#efe0f7]/60">
                <Plus className="h-3 w-3 transition-transform group-open:rotate-45" />
                Create another feed for this pair
              </summary>
              <div className="mt-3">
                <CreateFeedCard
                  collateralMint={collateralMint!}
                  lendMint={lendMint!}
                  collateralSymbol={collateralSymbol}
                  lendSymbol={lendSymbol}
                />
              </div>
            </details>
          </div>
        ) : isFetched && collateralMint && lendMint ? (
          <div className="flex flex-col gap-3">
            <p className="text-[11px] text-[#efe0f7]/30">
              No feeds declared for this pair.
            </p>
            <CreateFeedCard
              collateralMint={collateralMint}
              lendMint={lendMint}
              collateralSymbol={collateralSymbol}
              lendSymbol={lendSymbol}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ─── page ─────────────────────────────────────────────────────────────────────

export function FeedPage() {
  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      <div className="flex items-center gap-3 mb-8">
        <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-[#c698e5]/15">
          <Radio className="h-5 w-5 text-[#c698e5]" />
        </span>
        <h1 className="text-3xl font-semibold tracking-tight text-[#efe0f7]">
          Feed
        </h1>
      </div>

      <div className="w-2/3 flex flex-col gap-5">
        <FeedPusherTool />
        <FeedFinderTool />
      </div>
    </div>
  );
}

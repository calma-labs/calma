import { price_scale } from '@calma/wasm-lib'
import * as anchor from "@coral-xyz/anchor";
import { useCreateFeed } from "@/hooks/program/useCreateFeed";
import { useSetFeedFromPyth } from "@/hooks/program/useSetFeedFromPyth";
import { useSetFeedManualValue } from "@/hooks/program/useSetFeedManualValue";
import { refreshFeedAccount } from "@/hooks/program/refreshFeedForDevnet";
import { useFeedsByPair, type FeedByPair } from "@/hooks/program/useFeedsByPair";
import { usePythPrice } from "@/hooks/usePythPrice";
import { usePythFeeds } from "@/hooks/usePythFeeds";
import { type FeedRulesInput, noRules } from "@/config/feedRules";
import { pythQueryForToken, bytesToFeedIdHex } from "@/config/pythFeeds";
import { TokenSelect } from "@/components/ui/token-select";
import { connection, feedProgram } from "@/lib/program";
import { MINTER_KEYPAIR } from "@/store/wallet.store";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey, SendTransactionError, Transaction } from "@solana/web3.js";
import {
  Activity,
  CheckCircle2,
  Loader2,
  Plus,
  Send,
  Wallet,
} from "lucide-react";
import { useMemo, useState } from "react";

// ─── helpers ──────────────────────────────────────────────────────────────────

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

function shorten(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

const PRICE_SCALE = price_scale();

function formatRatio(collateral: bigint, lend: bigint): string {
  if (lend === 0n) return "—";
  const scaled = (collateral * PRICE_SCALE) / lend;
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

function feedSelectOptions(
  feeds: { id: string; name: string; icon: string }[] | undefined,
) {
  return (feeds ?? []).map((f) => ({
    address: f.id,
    symbol: f.name,
    icon: f.icon,
  }));
}


// ─── live Pyth prices for a feed pair ────────────────────────────────────────

function FeedLivePrices({
  collateralFeedId,
  lendFeedId,
}: {
  collateralFeedId: number[];
  lendFeedId: number[];
}) {
  const collateralHex = useMemo(() => bytesToFeedIdHex(collateralFeedId), [collateralFeedId]);
  const lendHex = useMemo(() => bytesToFeedIdHex(lendFeedId), [lendFeedId]);

  const { data: collPrice, isLoading: cLoading } = usePythPrice(collateralHex);
  const { data: lendPr, isLoading: lLoading } = usePythPrice(lendHex);

  const loading = cLoading || lLoading;
  const liveRatio =
    collPrice && lendPr && lendPr.price > 0n ? Number(collPrice.price) / Number(lendPr.price) : null;

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

// ─── feed refresh rows ────────────────────────────────────────────────────────

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
            done ? "text-[#34d399]" : isBusy ? "text-[#efe0f7]/80" : "text-[#efe0f7]/50",
          )}
        >
          {label}
          {signatures.length > 1 && (
            <span className="ml-1 text-[#efe0f7]/35">({signatures.length} tx)</span>
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

// ─── Pyth feed updater ────────────────────────────────────────────────────────

function PythFeedUpdater({
  feed,
  collateralMint,
  lendMint,
}: {
  feed: FeedByPair;
  collateralMint: PublicKey;
  lendMint: PublicKey;
}) {
  const { connected } = useWalletConnection();
  const { collateralAccount, lendAccount, busy, signatures, error, loadSide, commit, reset } =
    useSetFeedFromPyth();
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
    collateralAccount !== null || lendAccount !== null || signatures.commit.length > 0;

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
        const anchor = e.logs?.find(
          (l: string) => l.includes("Error Code:") || l.includes("AnchorError"),
        );
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
            {bgPending ? <Loader2 className="h-3 w-3 animate-spin" /> : <Send className="h-3 w-3" />}
            {bgPending ? "Sending…" : "Run as MINTER"}
          </button>
          {anythingLoaded && (
            <button
              type="button"
              onClick={reset}
              className="text-[10px] uppercase tracking-wider text-[#efe0f7]/40 hover:text-[#efe0f7]/70"
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

// ─── manual feed updater ──────────────────────────────────────────────────────

const PRICE_SCALE_N = price_scale();

function usdToScaledBn(field: string): anchor.BN | null {
  const s = field.trim();
  if (s === "") return new anchor.BN(0);
  const n = Number(s);
  if (!Number.isFinite(n) || n < 0) return null;
  if (n === 0) return new anchor.BN(0);
  const scaled = BigInt(Math.round(n * Number(PRICE_SCALE_N)));
  return new anchor.BN(scaled.toString());
}

function ManualFeedUpdater({
  feed,
  collateralMint,
  lendMint,
}: {
  feed: FeedByPair;
  collateralMint: PublicKey;
  lendMint: PublicKey;
}) {
  const { connected, wallet } = useWalletConnection();
  const walletPubkey = wallet ? new PublicKey(wallet.account.publicKey) : null;
  const isAuthority = !!walletPubkey && walletPubkey.equals(feed.authority);

  const [collateralUsd, setCollateralUsd] = useState("");
  const [lendUsd, setLendUsd] = useState("");
  const { mutateAsync, isPending, error, data: signature } = useSetFeedManualValue();
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
        const anchor = e.logs?.find(
          (l: string) => l.includes("Error Code:") || l.includes("AnchorError"),
        );
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
      <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">Push manual price</p>
      {!isAuthority ? (
        <p className="text-[10px] text-[#efe0f7]/35">
          Only the feed's authority ({shorten(feed.authority.toBase58())}) can update a Manual
          feed. Connect that wallet to push a price.
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
              {bgPending ? <Loader2 className="h-3 w-3 animate-spin" /> : <Send className="h-3 w-3" />}
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
              {isPending ? <Loader2 className="h-3 w-3 animate-spin" /> : <Send className="h-3 w-3" />}
              {isPending ? "Updating…" : "Update"}
            </button>
          </div>
          {bgError && <p className="text-[10px] text-[#d45677]">{bgError}</p>}
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

function FeedUpdaterActions({
  feed,
  collateralMint,
  lendMint,
}: {
  feed: FeedByPair;
  collateralMint: PublicKey;
  lendMint: PublicKey;
}) {
  if (feed.source === "Pyth") {
    return <PythFeedUpdater feed={feed} collateralMint={collateralMint} lendMint={lendMint} />;
  }
  if (feed.source === "Manual") {
    return <ManualFeedUpdater feed={feed} collateralMint={collateralMint} lendMint={lendMint} />;
  }
  return null;
}

// ─── rules form ───────────────────────────────────────────────────────────────

interface RulesInputStrings {
  maxConfBps: string;
  maxDeviationBpsPerHour: string;
  emaDivergenceBps: string;
  minPriceUsd: string;
  maxPriceUsd: string;
  maxAgeSecs: string;
}

function parseOptional(field: string): number | null {
  const s = field.trim();
  if (s === "") return 0;
  const n = Number(s);
  if (!Number.isFinite(n) || n < 0) return null;
  return n;
}

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
  if (min > 0 && max > 0 && min > max) return "Min price cannot exceed max price.";
  const age = Number(v.maxAgeSecs.trim());
  if (!Number.isFinite(age) || age <= 0) return "Max age must be a positive number (seconds).";
  return null;
}

function buildRulesInput(v: RulesInputStrings): FeedRulesInput {
  return {
    ...noRules(),
    maxConfBps: parseOptional(v.maxConfBps) ?? 0,
    maxDeviationBpsPerHour: parseOptional(v.maxDeviationBpsPerHour) ?? 0,
    emaDivergenceBps: parseOptional(v.emaDivergenceBps) ?? 0,
    minPrice: usdToScaledBn(v.minPriceUsd) ?? new anchor.BN(0),
    maxPrice: usdToScaledBn(v.maxPriceUsd) ?? new anchor.BN(0),
    maxAgeMs: Math.round(Number(v.maxAgeSecs.trim()) * 1000),
  };
}

function RuleInput({
  label,
  hint,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-[10px] uppercase tracking-wider text-[#efe0f7]/40">{label}</label>
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

// ─── create feed card ─────────────────────────────────────────────────────────

function CreateFeedCard({
  collateralMint,
  lendMint,
  collateralSymbol,
  lendSymbol,
}: {
  collateralMint: PublicKey;
  lendMint: PublicKey;
  collateralSymbol: string;
  lendSymbol: string;
}) {
  const { connected } = useWalletConnection();

  const { data: collateralFeeds } = usePythFeeds(
    collateralSymbol ? pythQueryForToken(collateralSymbol) : null,
  );
  const { data: lendFeeds } = usePythFeeds(
    lendSymbol ? pythQueryForToken(lendSymbol) : null,
  );

  const [collateralChoice, setCollateralChoice] = useState("");
  const [lendChoice, setLendChoice] = useState("");

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
    await mutateAsync({ collateralMint, lendMint, collateralFeedId, lendFeedId, rules });
  }

  return (
    <div className="rounded-xl border border-[#c698e5]/20 bg-[#c698e5]/[0.04] p-5">
      <div className="flex items-center gap-2 mb-4">
        <Plus className="h-4 w-4 text-[#c698e5]" />
        <h3 className="text-sm font-semibold text-[#efe0f7]/80">Create a feed for this pair</h3>
      </div>

      <div className="flex flex-col gap-4">
        {/* Pyth feed selectors */}
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
            All fields are optional. Leave empty (or 0) to disable a rule. Rules are fixed at
            create time and enforced when this feed is refreshed via{" "}
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
          {rulesError && <p className="mt-3 text-[11px] text-[#d45677]">{rulesError}</p>}
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
          <button
            type="button"
            onClick={handleCreate}
            disabled={!connected || isPending || !feedsResolved || !!rulesError}
            className={cn(
              "flex items-center gap-2 rounded-xl px-5 py-2 text-sm font-semibold transition-all duration-200",
              "bg-[#c698e5] text-[#17081f]",
              "enabled:hover:brightness-110 enabled:active:scale-95",
              "disabled:opacity-50 disabled:cursor-not-allowed",
            )}
          >
            {isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : !connected ? (
              <Wallet className="h-4 w-4" />
            ) : (
              <Plus className="h-4 w-4" />
            )}
            {isPending ? "Creating…" : !connected ? "Connect wallet" : "Create feed"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── feed card (existing feed display + actions) ──────────────────────────────

function FeedCard({
  feed,
  collateralMint,
  lendMint,
  selected,
  onSelect,
}: {
  feed: FeedByPair;
  collateralMint: PublicKey;
  lendMint: PublicKey;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <div
      className={cn(
        "rounded-xl border px-4 py-3 cursor-pointer transition-colors",
        selected
          ? "border-[#c698e5]/50 bg-[#c698e5]/[0.07]"
          : "border-[#c698e5]/15 bg-[#c698e5]/[0.03] hover:border-[#c698e5]/30",
      )}
      onClick={onSelect}
    >
      <div className="flex items-center justify-between gap-2 mb-2">
        <div className="flex items-center gap-2">
          {selected && (
            <CheckCircle2 className="h-3.5 w-3.5 text-[#c698e5] shrink-0" />
          )}
          <span className="font-mono text-xs text-[#efe0f7]/70 break-all">
            {shorten(feed.publicKey.toBase58())}
          </span>
        </div>
        <span
          className={cn(
            "rounded-md px-2 py-0.5 text-[10px] font-semibold shrink-0",
            feed.source === "Pyth"
              ? "bg-[#c698e5]/15 text-[#c698e5]"
              : "bg-[#efe0f7]/10 text-[#efe0f7]/50",
          )}
        >
          {feed.source}
        </span>
      </div>

      {/* on-chain prices */}
      <div className="grid grid-cols-3 gap-3 text-[11px]">
        <div className="flex flex-col gap-0.5">
          <span className="text-[#efe0f7]/30">On-chain ratio</span>
          <span className="font-mono text-[#efe0f7]/80 tabular-nums">
            {formatRatio(feed.collateralPrice, feed.lendPrice)}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <span className="text-[#efe0f7]/30">On-chain coll px</span>
          <span className="font-mono text-[#efe0f7]/60 tabular-nums">
            {formatScaled(feed.collateralPrice)}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <span className="text-[#efe0f7]/30">On-chain lend px</span>
          <span className="font-mono text-[#efe0f7]/60 tabular-nums">
            {formatScaled(feed.lendPrice)}
          </span>
        </div>
      </div>

      {/* live Pyth prices */}
      {(feed.source === "Pyth" || feed.source === "PythPush") && (
        <FeedLivePrices
          collateralFeedId={feed.collateralFeedId}
          lendFeedId={feed.lendFeedId}
        />
      )}

      <div className="mt-2 flex items-center justify-between text-[10px] text-[#efe0f7]/30">
        <span className="font-mono">auth {shorten(feed.authority.toBase58())}</span>
        <span>
          {feed.lastUpdatedTs > 0
            ? `updated ${relativeTime(feed.lastUpdatedTs)}`
            : "never updated"}
        </span>
      </div>

      {/* update actions — only shown when selected */}
      {selected && (
        <FeedUpdaterActions
          feed={feed}
          collateralMint={collateralMint}
          lendMint={lendMint}
        />
      )}
    </div>
  );
}

// ─── public API ───────────────────────────────────────────────────────────────

export interface FeedPairPanelProps {
  collateralMint: PublicKey;
  lendMint: PublicKey;
  collateralSymbol: string;
  lendSymbol: string;
}

export function FeedPairPanel({
  collateralMint,
  lendMint,
  collateralSymbol,
  lendSymbol,
}: FeedPairPanelProps) {
  const { data: feeds, isLoading, error, isFetched } = useFeedsByPair(
    collateralMint,
    lendMint,
  );

  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const selectedFeed = useMemo(() => {
    if (!feeds || feeds.length === 0) return null;
    if (selectedKey) return feeds.find((f) => f.publicKey.toBase58() === selectedKey) ?? feeds[0];
    return feeds[0];
  }, [feeds, selectedKey]);

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-[#efe0f7]/40 text-sm py-2">
        <Loader2 className="h-4 w-4 animate-spin" /> Querying feeds…
      </div>
    );
  }

  if (error) {
    return (
      <p className="text-sm text-[#d45677]">
        {error instanceof Error ? error.message : "Failed to load feeds"}
      </p>
    );
  }

  if (feeds && feeds.length > 0) {
    return (
      <div className="flex flex-col gap-3">
        <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
          {feeds.length} feed{feeds.length === 1 ? "" : "s"} found · click to select
        </p>

        {feeds.map((f) => (
          <FeedCard
            key={f.publicKey.toBase58()}
            feed={f}
            collateralMint={collateralMint}
            lendMint={lendMint}
            selected={f === selectedFeed}
            onSelect={() => setSelectedKey(f.publicKey.toBase58())}
          />
        ))}

        <details className="group">
          <summary className="flex cursor-pointer list-none items-center gap-2 py-1 text-[10px] uppercase tracking-wider text-[#efe0f7]/35 hover:text-[#efe0f7]/60">
            <Plus className="h-3 w-3 transition-transform group-open:rotate-45" />
            Create another feed for this pair
          </summary>
          <div className="mt-3">
            <CreateFeedCard
              collateralMint={collateralMint}
              lendMint={lendMint}
              collateralSymbol={collateralSymbol}
              lendSymbol={lendSymbol}
            />
          </div>
        </details>
      </div>
    );
  }

  if (isFetched) {
    return (
      <div className="flex flex-col gap-3">
        <p className="text-[11px] text-[#efe0f7]/30">No feeds declared for this pair.</p>
        <CreateFeedCard
          collateralMint={collateralMint}
          lendMint={lendMint}
          collateralSymbol={collateralSymbol}
          lendSymbol={lendSymbol}
        />
      </div>
    );
  }

  return null;
}

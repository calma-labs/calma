import { ActionButton } from "@/components/common/ActionButton";
import { TokenSelect } from "@/components/ui/token-select";
import { usePushPythFeed } from "@/hooks/program/usePushPythFeed";
import { useFeedsByPair } from "@/hooks/program/useFeedsByPair";
import { useCreateFeed } from "@/hooks/program/useCreateFeed";
import { usePythPrice } from "@/hooks/usePythPrice";
import { usePythFeeds } from "@/hooks/usePythFeeds";
import { pythQueryForToken } from "@/config/pythFeeds";
import { getTokenOptions } from "@/config/poolRegistry";
import { feedPda } from "@/lib/program";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
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

// ─── feed pusher tool ──────────────────────────────────────────────────────────

function FeedPusherTool() {
  const { connected, wallet } = useWalletConnection();

  // Pick a token; its Pyth feed is fetched from Hermes' catalog (1:1 per token).
  const [tokenAddr, setTokenAddr] = useState(TOKEN_OPTIONS[0]?.address ?? "");
  const tokenSymbol =
    TOKEN_OPTIONS.find((o) => o.address === tokenAddr)?.symbol ?? "";
  const query = tokenSymbol ? pythQueryForToken(tokenSymbol) : null;

  const {
    data: feeds,
    isLoading: feedsLoading,
    error: feedsError,
  } = usePythFeeds(query);

  // The catalog usually returns one USD feed; if several, let the user choose.
  // `feedId` is the explicit choice; fall back to the first match otherwise.
  const [feedId, setFeedId] = useState("");
  const feed = feeds?.find((f) => f.id === feedId) ?? feeds?.[0] ?? null;

  const { data: price, isLoading: priceLoading, error: priceError } =
    usePythPrice(feed ? feed.id : null);

  const { mutateAsync, isPending, error, data } = usePushPythFeed();

  const feedAddress =
    connected && wallet
      ? feedPda(new PublicKey(wallet.account.publicKey)).toBase58()
      : null;

  async function handleSend() {
    if (!feed) return;
    await mutateAsync({ feed });
  }

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          <Radio className="h-4 w-4" />
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">
          Push Pyth Price
        </h2>
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        Select a token — its Pyth feed is looked up live from the Hermes catalog
        — watch the price, then post a signed price update to your wallet's{" "}
        <span className="font-mono">feed</span> account (priced against USDC/USD).
        Creates the feed on first use.
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

        {/* live price */}
        {feed && (
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
                  {formatPrice(price.price)}
                </span>
                <span className="text-[11px] text-[#efe0f7]/35">
                  ± {formatPrice(price.confidence)} · {relativeTime(price.publishTime)}
                </span>
              </div>
            ) : null}
          </div>
        )}

        {/* target feed account */}
        {feedAddress && (
          <div className="flex flex-col gap-1">
            <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
              Target feed account
            </p>
            <p className="font-mono text-xs text-[#efe0f7]/60 break-all">
              {feedAddress}
            </p>
          </div>
        )}

        {error && (
          <p className="text-[11px] text-[#d45677]">
            {error instanceof Error ? error.message : "Unknown error"}
          </p>
        )}

        {data && (
          <div className="rounded-xl border border-[#34d399]/25 bg-[#34d399]/5 px-4 py-3">
            <div className="flex items-center gap-1.5 mb-1">
              <CheckCircle2 className="h-3.5 w-3.5 text-[#34d399]" />
              <p className="text-xs text-[#34d399]">
                {data.created ? "Feed created & priced" : "Price pushed"} ·{" "}
                {data.signatures.length} tx
              </p>
            </div>
            <div className="flex flex-col gap-0.5">
              {data.signatures.map((sig) => (
                <a
                  key={sig}
                  href={`https://solscan.io/tx/${sig}?cluster=devnet`}
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
            disabled={
              !connected || isPending || priceLoading || !!priceError || !feed
            }
            label={
              isPending ? "Sending…" : !connected ? "Connect wallet" : "Send pull oracle"
            }
            icon={
              isPending ? (
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

// ─── feed finder (by mint pair) ─────────────────────────────────────────────────

/** Prices on a feed are stored scaled by 1e6 (see `PRICE_SCALE`). */
const PRICE_SCALE = 1_000_000n;

function shorten(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

/** collateral / lend, formatted as a plain ratio. */
function formatRatio(collateral: bigint, lend: bigint): string {
  if (lend === 0n) return "—";
  const scaled = (collateral * 1_000_000n) / lend; // 6 fractional digits
  return (Number(scaled) / 1_000_000).toLocaleString("en-US", {
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
    collateralPrice && lendPrice && lendPrice.price > 0
      ? collateralPrice.price / lendPrice.price
      : null;

  const feedsResolved = !!collateralFeedId && !!lendFeedId;

  async function handleCreate() {
    if (!feedsResolved) return;
    await mutateAsync({ collateralMint, lendMint, collateralFeedId, lendFeedId });
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
                {collateralPrice ? formatPrice(collateralPrice.price) : "—"}
              </span>
            </div>
            <div className="flex flex-col gap-0.5">
              <span className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30">
                {lendSymbol || "Lend"}
              </span>
              <span className="font-mono text-sm text-[#efe0f7]/80 tabular-nums">
                {lendPrice ? formatPrice(lendPrice.price) : "—"}
              </span>
            </div>
          </div>
        </div>

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
            disabled={!connected || isPending || !feedsResolved}
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
                    <span className="text-[#efe0f7]/30">Ratio (coll/lend)</span>
                    <span className="font-mono text-[#efe0f7]/80 tabular-nums">
                      {formatRatio(f.collateralPrice, f.lendPrice)}
                    </span>
                  </div>
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[#efe0f7]/30">Collateral px</span>
                    <span className="font-mono text-[#efe0f7]/60 tabular-nums">
                      {formatScaled(f.collateralPrice)}
                    </span>
                  </div>
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[#efe0f7]/30">Lend px</span>
                    <span className="font-mono text-[#efe0f7]/60 tabular-nums">
                      {formatScaled(f.lendPrice)}
                    </span>
                  </div>
                </div>
                <div className="mt-2 flex items-center justify-between text-[10px] text-[#efe0f7]/30">
                  <span className="font-mono">auth {shorten(f.authority.toBase58())}</span>
                  <span>
                    {f.lastUpdatedTs > 0
                      ? `updated ${relativeTime(f.lastUpdatedTs)}`
                      : "never updated"}
                  </span>
                </div>
              </div>
            ))}
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

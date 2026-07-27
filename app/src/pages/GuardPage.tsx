import { ActionButton } from "@/components/common/ActionButton";
import { useAddToWaitlist } from "@/hooks/program/useAddToWaitlist";
import { useGuardState } from "@/hooks/program/useGuardState";
import { useSimulateCheck } from "@/hooks/program/useSimulateCheck";
import { cn } from "@/lib/utils";
import { useWalletConnection } from "@solana/react-hooks";
import { PublicKey } from "@solana/web3.js";
import {
  CheckCircle2,
  Loader2,
  Play,
  Search,
  ShieldCheck,
  UserPlus,
  Wallet,
  XCircle,
} from "lucide-react";
import { useMemo, useState } from "react";

function shorten(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

/** Parse a base58 pubkey, returning null when the string is empty or invalid. */
function parsePubkey(value: string): PublicKey | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  try {
    return new PublicKey(trimmed);
  } catch {
    return null;
  }
}

const inputClass =
  "w-full rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-2.5 font-mono text-xs text-[#efe0f7]/80 placeholder:text-[#efe0f7]/25 outline-none focus:border-[#c698e5]/40 transition-colors";

// ─── add to waitlist ────────────────────────────────────────────────────────────

function AddToWaitlistTool() {
  const { connected, wallet } = useWalletConnection();
  const [value, setValue] = useState("");

  const authority = useMemo(
    () => (connected && wallet ? new PublicKey(wallet.account.publicKey) : null),
    [connected, wallet],
  );

  const { data: guard, isLoading: guardLoading } = useGuardState(authority);
  const { mutateAsync, isPending, error } = useAddToWaitlist();

  const pubkey = parsePubkey(value);
  const invalid = value.trim().length > 0 && !pubkey;
  const alreadyIn =
    pubkey && guard?.whitelist.some((k) => k.equals(pubkey));

  async function handleAdd() {
    if (!pubkey) return;
    await mutateAsync({ pubkey });
    setValue("");
  }

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          <UserPlus className="h-4 w-4" />
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">
          Add to Waitlist
        </h2>
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        Add a pubkey to your wallet's <span className="font-mono">guard</span>{" "}
        whitelist. Each wallet owns its own guard PDA — the first add also
        creates the account.
      </p>

      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Pubkey to whitelist
          </label>
          <div className="flex gap-2">
            <input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="Base58 address…"
              className={inputClass}
            />
            <ActionButton
              label="Me"
              variant="secondary"
              compact
              onClick={() =>
                authority && setValue(authority.toBase58())
              }
              disabled={!authority}
              className="shrink-0"
              title="Fill with connected wallet"
              icon={<Wallet className="h-3.5 w-3.5 text-[#c698e5]" />}
            />
          </div>
          {invalid && (
            <p className="text-[11px] text-[#d45677]">Invalid base58 pubkey.</p>
          )}
          {alreadyIn && (
            <p className="text-[11px] text-[#e0b64d]">
              Already on your waitlist.
            </p>
          )}
        </div>

        {/* current whitelist */}
        <div className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-3">
          <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30 mb-2">
            Your waitlist
            {guard ? ` · ${guard.whitelist.length}` : ""}
          </p>
          {!authority ? (
            <p className="text-[11px] text-[#efe0f7]/30">
              Connect a wallet to view your waitlist.
            </p>
          ) : guardLoading ? (
            <div className="flex items-center gap-2 text-[#efe0f7]/40 text-xs">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> Loading…
            </div>
          ) : !guard ? (
            <p className="text-[11px] text-[#efe0f7]/30">
              No guard created yet — adding an entry will create one.
            </p>
          ) : guard.whitelist.length === 0 ? (
            <p className="text-[11px] text-[#efe0f7]/30">Waitlist is empty.</p>
          ) : (
            <div className="flex flex-col gap-1">
              {guard.whitelist.map((k) => (
                <span
                  key={k.toBase58()}
                  className="font-mono text-[11px] text-[#efe0f7]/70 break-all"
                >
                  {k.toBase58()}
                </span>
              ))}
            </div>
          )}
        </div>

        {error && (
          <p className="text-[11px] text-[#d45677]">
            {error instanceof Error ? error.message : "Unknown error"}
          </p>
        )}

        <div className="flex justify-end pt-1">
          <ActionButton
            variant="primary"
            onClick={handleAdd}
            disabled={!connected || isPending || !pubkey || !!alreadyIn}
            label={
              isPending ? "Adding…" : !connected ? "Connect wallet" : "Add to waitlist"
            }
            icon={
              isPending ? (
                <Loader2 className="h-4 w-4 animate-spin text-[#17081f]" />
              ) : !connected ? (
                <Wallet className="h-4 w-4 text-[#17081f]" />
              ) : (
                <UserPlus className="h-4 w-4 text-[#17081f]" />
              )
            }
          />
        </div>
      </div>
    </div>
  );
}

// ─── check membership ─────────────────────────────────────────────────────────

function CheckMembershipTool() {
  const { connected, wallet } = useWalletConnection();
  const [authorityValue, setAuthorityValue] = useState("");
  const [pubkeyValue, setPubkeyValue] = useState("");

  const walletKey = useMemo(
    () => (connected && wallet ? new PublicKey(wallet.account.publicKey) : null),
    [connected, wallet],
  );

  // Guard authority to inspect — defaults to the connected wallet when blank.
  const authority = useMemo(() => {
    const parsed = parsePubkey(authorityValue);
    if (parsed) return parsed;
    if (authorityValue.trim().length === 0) return walletKey;
    return null;
  }, [authorityValue, walletKey]);

  const pubkey = parsePubkey(pubkeyValue);

  const { data: guard, isLoading: guardLoading } = useGuardState(authority);
  const {
    mutateAsync: simulate,
    data: simResult,
    isPending: simulating,
    error: simError,
    reset,
  } = useSimulateCheck();

  const authorityInvalid =
    authorityValue.trim().length > 0 && !parsePubkey(authorityValue);
  const pubkeyInvalid = pubkeyValue.trim().length > 0 && !pubkey;

  // Read-only client-side verdict from the fetched whitelist.
  const readVerdict =
    pubkey && guard
      ? guard.whitelist.some((k) => k.equals(pubkey))
      : null;

  async function handleSimulate() {
    if (!authority || !pubkey) return;
    await simulate({ authority, pubkey });
  }

  return (
    <div className="rounded-2xl border border-[#c698e5]/12 bg-[#c698e5]/[0.025] p-6">
      <div className="flex items-center gap-2.5 mb-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-[#c698e5]/15 text-[#c698e5]">
          <Search className="h-4 w-4" />
        </span>
        <h2 className="text-sm font-semibold text-[#efe0f7]/80">
          Check Membership
        </h2>
      </div>

      <p className="text-[11px] text-[#efe0f7]/35 mb-5">
        Read a guard's whitelist directly, or run the on-chain{" "}
        <span className="font-mono">check</span> instruction as a simulated
        transaction from your attached wallet (no fee, no state change).
      </p>

      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Guard authority
          </label>
          <input
            value={authorityValue}
            onChange={(e) => {
              setAuthorityValue(e.target.value);
              reset();
            }}
            placeholder={
              walletKey
                ? `${shorten(walletKey.toBase58())} (your wallet)`
                : "Guard owner address…"
            }
            className={inputClass}
          />
          {authorityInvalid && (
            <p className="text-[11px] text-[#d45677]">Invalid base58 pubkey.</p>
          )}
        </div>

        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-semibold uppercase tracking-wider text-[#efe0f7]/45">
            Address to check
          </label>
          <div className="flex gap-2">
            <input
              value={pubkeyValue}
              onChange={(e) => {
                setPubkeyValue(e.target.value);
                reset();
              }}
              placeholder="Base58 address…"
              className={inputClass}
            />
            <ActionButton
              label="Me"
              variant="secondary"
              compact
              onClick={() =>
                walletKey && setPubkeyValue(walletKey.toBase58())
              }
              disabled={!walletKey}
              className="shrink-0"
              title="Fill with connected wallet"
              icon={<Wallet className="h-3.5 w-3.5 text-[#c698e5]" />}
            />
          </div>
          {pubkeyInvalid && (
            <p className="text-[11px] text-[#d45677]">Invalid base58 pubkey.</p>
          )}
        </div>

        {/* client-side read verdict */}
        {pubkey && authority && (
          <div className="rounded-xl border border-[#c698e5]/15 bg-[#c698e5]/[0.03] px-4 py-3">
            <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30 mb-1.5">
              Account read
            </p>
            {guardLoading ? (
              <div className="flex items-center gap-2 text-[#efe0f7]/40 text-xs">
                <Loader2 className="h-3.5 w-3.5 animate-spin" /> Reading guard…
              </div>
            ) : !guard ? (
              <p className="text-[11px] text-[#efe0f7]/40">
                No guard account for this authority.
              </p>
            ) : readVerdict ? (
              <div className="flex items-center gap-1.5 text-[#34d399] text-sm">
                <CheckCircle2 className="h-4 w-4" /> On the waitlist
              </div>
            ) : (
              <div className="flex items-center gap-1.5 text-[#d45677] text-sm">
                <XCircle className="h-4 w-4" /> Not on the waitlist
              </div>
            )}
          </div>
        )}

        {/* simulation result */}
        {(simResult || simError) && (
          <div
            className={cn(
              "rounded-xl border px-4 py-3",
              simResult?.whitelisted
                ? "border-[#34d399]/25 bg-[#34d399]/5"
                : "border-[#d45677]/25 bg-[#d45677]/5",
            )}
          >
            <p className="text-[10px] uppercase tracking-wider text-[#efe0f7]/30 mb-1.5">
              Simulated on-chain check
            </p>
            {simError ? (
              <p className="text-sm text-[#d45677]">
                {simError instanceof Error ? simError.message : "Simulation failed"}
              </p>
            ) : simResult?.whitelisted ? (
              <div className="flex items-center gap-1.5 text-[#34d399] text-sm mb-2">
                <CheckCircle2 className="h-4 w-4" /> check passed — whitelisted
              </div>
            ) : (
              <div className="flex items-center gap-1.5 text-[#d45677] text-sm mb-2">
                <XCircle className="h-4 w-4" /> {simResult?.error}
              </div>
            )}
            {simResult && simResult.logs.length > 0 && (
              <pre className="mt-1 max-h-40 overflow-auto rounded-lg bg-[#17081f]/60 p-3 font-mono text-[10px] leading-relaxed text-[#efe0f7]/45">
                {simResult.logs.join("\n")}
              </pre>
            )}
          </div>
        )}

        <div className="flex justify-end pt-1">
          <ActionButton
            variant="primary"
            onClick={handleSimulate}
            disabled={!connected || simulating || !authority || !pubkey}
            label={
              simulating ? "Simulating…" : !connected ? "Connect wallet" : "Simulate check"
            }
            icon={
              simulating ? (
                <Loader2 className="h-4 w-4 animate-spin text-[#17081f]" />
              ) : !connected ? (
                <Wallet className="h-4 w-4 text-[#17081f]" />
              ) : (
                <Play className="h-4 w-4 text-[#17081f]" />
              )
            }
          />
        </div>
      </div>
    </div>
  );
}

// ─── page ─────────────────────────────────────────────────────────────────────

export function GuardPage() {
  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      <div className="flex items-center gap-3 mb-8">
        <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-[#c698e5]/15">
          <ShieldCheck className="h-5 w-5 text-[#c698e5]" />
        </span>
        <h1 className="text-3xl font-semibold tracking-tight text-[#efe0f7]">
          Guard
        </h1>
      </div>

      <div className="w-2/3 flex flex-col gap-5">
        <AddToWaitlistTool />
        <CheckMembershipTool />
      </div>
    </div>
  );
}

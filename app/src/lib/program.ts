import { AnchorProvider, Program } from "@anchor-lang/core";
import { Connection, PublicKey } from "@solana/web3.js";
import { Buffer } from "buffer";
import IDL from "../../../target/idl/calma.json";
import type { Calma } from "../../../target/types/calma";
import IRM_IDL from "../../../target/idl/irm.json";
import type { Irm } from "../../../target/types/irm";
import FEED_IDL from "../../../target/idl/feed.json";
import type { Feed } from "../../../target/types/feed";
import GUARD_IDL from "../../../target/idl/guard.json";
import FAUCET_IDL from "../../../target/idl/faucet.json";
import type { Guard } from "../../../target/types/guard";
import type { Faucet } from "../../../target/types/faucet";

if (typeof window !== "undefined" && !window.Buffer) {
  window.Buffer = Buffer;
}

const endpoint =
  import.meta.env.VITE_SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

const wsEndpoint: string | undefined = import.meta.env.VITE_SOLANA_WS_URL;

export const connection = new Connection(endpoint, {
  commitment: "confirmed",
  ...(wsEndpoint ? { wsEndpoint } : {}),
});

// Read-only provider — no real wallet needed for data fetching
const readOnlyProvider = new AnchorProvider(
  connection,
  // Wallet stub: satisfies the interface without holding a keypair
  { publicKey: null as any, signTransaction: null as any, signAllTransactions: null as any },
  { commitment: "confirmed" }
);

export const program = new Program<Calma>(IDL as unknown as Calma, readOnlyProvider);
export const irmProgram = new Program<Irm>(IRM_IDL as unknown as Irm, readOnlyProvider);
export const feedProgram = new Program<Feed>(FEED_IDL as unknown as Feed, readOnlyProvider);
export const guardProgram = new Program<Guard>(GUARD_IDL as unknown as Guard, readOnlyProvider);
/** Test-only faucet, split out of the calma program so production never ships it. */
export const faucetProgram = new Program<Faucet>(FAUCET_IDL as unknown as Faucet, readOnlyProvider);

export const IRM_PROGRAM_ID = new PublicKey(
  "irmdacogiedKeCEBh72FJx4aoixyaByqGikTkxGifUk"
);

export const FEED_PROGRAM_ID = new PublicKey(
  "orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4"
);

export const GUARD_PROGRAM_ID = new PublicKey(
  "grddH13wp77vjwV2WwzbVXAkgRGQuTHkj1hKcECtHRt"
);

export const FAUCET_PROGRAM_ID = new PublicKey(
  "HALzjfshwyYKYjLNL6ectL9oBoM3tLcUCAabZNYrxwWy"
);

/**
 * PDA of the `feed` account for a given (collateral mint, lend mint, id) triple.
 * Seeds: `["feed", collateral_mint, lend_mint, id]`.
 */
export function feedPda(
  collateralMint: PublicKey,
  lendMint: PublicKey,
  id = 0,
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("feed"),
      collateralMint.toBuffer(),
      lendMint.toBuffer(),
      Buffer.from([id]),
    ],
    FEED_PROGRAM_ID,
  )[0];
}

/** PDA of the `irm_config` account for a given pool (seeds: ["irm_config", pool]). */
export function irmStatePda(pool: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("irm_config"), pool.toBuffer()],
    IRM_PROGRAM_ID,
  )[0];
}

/**
 * PDA of the protocol's single `guard_state` whitelist (seeds: ["guard"]).
 *
 * Formerly seeded per-authority, which made whitelists permissionless and
 * per-caller — anyone could create one naming themselves and present it to a
 * consumer that only verified the guard *program*. There is now exactly one.
 */
/**
 * Whitelist owned by `authority`. Guards are per-authority, so several coexist
 * over different subsets — a market's own list is `Pool.guardState`, not
 * whatever this derives for the connected wallet.
 */
export function guardPda(authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("guard"), authority.toBuffer()],
    GUARD_PROGRAM_ID
  )[0];
}

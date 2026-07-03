import { AnchorProvider, Program } from "@anchor-lang/core";
import { Connection, PublicKey } from "@solana/web3.js";
import { Buffer } from "buffer";
import IDL from "../../../target/idl/jbl.json";
import type { Jbl } from "../../../target/types/jbl";
import IRM_IDL from "../../../target/idl/irm.json";
import type { Irm } from "../../../target/types/irm";
import FEED_IDL from "../../../target/idl/feed.json";
import type { Feed } from "../../../target/types/feed";
import GUARD_IDL from "../../../target/idl/guard.json";
import type { Guard } from "../../../target/types/guard";

if (typeof window !== "undefined" && !window.Buffer) {
  window.Buffer = Buffer;
}

const endpoint =
  import.meta.env.VITE_SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

export const connection = new Connection(endpoint, "confirmed");

// Read-only provider — no real wallet needed for data fetching
const readOnlyProvider = new AnchorProvider(
  connection,
  // Wallet stub: satisfies the interface without holding a keypair
  { publicKey: null as any, signTransaction: null as any, signAllTransactions: null as any },
  { commitment: "confirmed" }
);

export const program = new Program<Jbl>(IDL as unknown as Jbl, readOnlyProvider);
export const irmProgram = new Program<Irm>(IRM_IDL as unknown as Irm, readOnlyProvider);
export const feedProgram = new Program<Feed>(FEED_IDL as unknown as Feed, readOnlyProvider);
export const guardProgram = new Program<Guard>(GUARD_IDL as unknown as Guard, readOnlyProvider);

export const FEED_PROGRAM_ID = new PublicKey(
  "orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4"
);

export const GUARD_PROGRAM_ID = new PublicKey(
  "grddH13wp77vjwV2WwzbVXAkgRGQuTHkj1hKcECtHRt"
);

/** PDA of the `feed` account owned by `authority` (seeds: ["feed", authority]). */
export function feedPda(authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("feed"), authority.toBuffer()],
    FEED_PROGRAM_ID
  )[0];
}

/** PDA of the `guard_state` account owned by `authority` (seeds: ["guard", authority]). */
export function guardPda(authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("guard"), authority.toBuffer()],
    GUARD_PROGRAM_ID
  )[0];
}

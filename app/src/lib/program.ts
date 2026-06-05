import { AnchorProvider, Program } from "@anchor-lang/core";
import { Connection } from "@solana/web3.js";
import { Buffer } from "buffer";
import IDL from "../../../target/idl/jbl.json";
import type { Jbl } from "../../../target/types/jbl";
import IRM_IDL from "../../../target/idl/irm.json";
import type { Irm } from "../../../target/types/irm";

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

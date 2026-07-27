import { PublicKey } from "@solana/web3.js";
import { getMint } from "@solana/spl-token";
import { connection } from "./program";
import { MINTER_PUBKEY } from "../store/wallet.store";
import type { PoolAccountWithKey } from "../hooks/program/useLendingAccounts";
import { getTokenMeta } from "./tokenRegistry";

/**
 * Hardcoded blacklist of token mint addresses.
 * Add any tokens that should be hidden from the UI here.
 */
export const TOKEN_BLACKLIST = new Set<string>([
  // Example: "TokenMintAddress111111111111111111111111111",
]);

/**
 * Hardcoded blacklist of pool addresses.
 * Add any pools that should be hidden from the UI here.
 */
export const POOL_BLACKLIST = new Set<string>([
  // Example: "PoolAddress111111111111111111111111111111",
]);

/**
 * Checks if a token is valid (not blacklisted and has the correct faucet minter).
 */
export async function isTokenValid(mint: PublicKey): Promise<boolean> {
  const address = mint.toBase58();
  if (TOKEN_BLACKLIST.has(address)) return false;

  try {
    const info = await getMint(connection, mint);
    return info.mintAuthority?.toBase58() === MINTER_PUBKEY.toBase58();
  } catch {
    return false;
  }
}

/**
 * Both collateral and lend mints must appear in the hardcoded token registry.
 */
function poolTokensAreRegistered(pool: PoolAccountWithKey): boolean {
  const collateral = new PublicKey(pool.account.collateral_mint).toBase58();
  const lend = new PublicKey(pool.account.lend_mint).toBase58();
  return getTokenMeta(collateral) !== null && getTokenMeta(lend) !== null;
}

/**
 * Checks if a pool is valid (not blacklisted, tokens are registered, and
 * both mints are valid faucets).
 */
export async function isPoolValid(pool: PoolAccountWithKey): Promise<boolean> {
  if (POOL_BLACKLIST.has(pool.publicKey.toBase58())) return false;
  if (!poolTokensAreRegistered(pool)) return false;

  const [collateralValid, lendValid] = await Promise.all([
    isTokenValid(new PublicKey(pool.account.collateral_mint)),
    isTokenValid(new PublicKey(pool.account.lend_mint)),
  ]);

  return collateralValid && lendValid;
}

/**
 * Checks if a pool is valid specifically for multiply strategies.
 * For multiply, we allow the pool if AT LEAST ONE of its tokens is a valid faucet.
 */
export async function isMultiplyPoolValid(pool: PoolAccountWithKey): Promise<boolean> {
  if (POOL_BLACKLIST.has(pool.publicKey.toBase58())) return false;
  if (!poolTokensAreRegistered(pool)) return false;

  const [collateralValid, lendValid] = await Promise.all([
    isTokenValid(new PublicKey(pool.account.collateral_mint)),
    isTokenValid(new PublicKey(pool.account.lend_mint)),
  ]);

  return collateralValid || lendValid;
}

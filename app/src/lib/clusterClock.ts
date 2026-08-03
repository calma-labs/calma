import { PublicKey } from '@solana/web3.js'
import { clock_sysvar_address, clock_unix_timestamp } from '@calma/wasm-lib'
import { connection } from './program'

/**
 * The cluster's `Clock::unix_timestamp` — the value on-chain code reads from
 * `Clock::get()`.
 *
 * Every wasm interest-accrual replay is stamped with this. It must not be
 * replaced with `Date.now()`: the user's machine clock can be skewed, and even a
 * perfectly set one disagrees with the chain, because Solana's `unix_timestamp`
 * is derived from validator vote timestamps rather than wall time and has
 * historically run behind it. A replay exists to predict what the program will
 * compute, so it has to be given what the program will read.
 *
 * Both the address and the byte offset come from Rust (`clock_sysvar_address`,
 * `clock_unix_timestamp`) rather than being written out here — see
 * `.claude/rules/wasm-bindings.md`.
 */
export const CLOCK_SYSVAR = new PublicKey(clock_sysvar_address())

/** Read the cluster clock. Throws if the sysvar cannot be read or decoded — a
 *  wrong timestamp would silently misprice every figure derived from it. */
export async function fetchClusterClockTs(): Promise<bigint> {
    const info = await connection.getAccountInfo(CLOCK_SYSVAR)
    if (!info) throw new Error('Clock sysvar account not found')
    const ts = clock_unix_timestamp(info.data)
    if (ts === undefined || ts === null) {
        throw new Error('Clock sysvar account did not decode')
    }
    return ts
}

import { PoolAccount } from '@calma/wasm-lib'
import { Connection, PublicKey } from '@solana/web3.js'

export interface GuardAccounts {
    guardProgram: PublicKey | null
    guardState: PublicKey | null
}

const UNGATED: GuardAccounts = { guardProgram: null, guardState: null }

/**
 * Resolve the guard accounts an entry instruction has to be sent with.
 *
 * `deposit_collateral`, `deposit_lent` and `borrow` take `guard_program` and
 * `guard_state` as *optional* accounts, and the market decides whether they are
 * required: an ungated pool ignores them, a gated one rejects the instruction
 * with `GuardRequired` if they are missing. So "optional" here means "depends on
 * the pool", not "the caller may skip them" — passing `null` unconditionally, as
 * this app used to, makes every gated market unusable from the UI.
 *
 * Both values come off the pool itself rather than from configuration. `calma`
 * links no guard implementation, so there is no canonical program id to assume:
 * a market records the program *and* the whitelist it chose at creation and
 * pins both, and substituting either is refused on-chain.
 */
export async function resolveGuardAccounts(
    connection: Connection,
    pool: PublicKey,
): Promise<GuardAccounts> {
    const info = await connection.getAccountInfo(pool)
    if (!info) return UNGATED

    const decoded = PoolAccount.from_bytes(new Uint8Array(info.data))
    if (!decoded) return UNGATED

    try {
        if (!decoded.is_guarded()) return UNGATED
        return {
            guardProgram: new PublicKey(decoded.guard_program),
            guardState: new PublicKey(decoded.guard_state),
        }
    } finally {
        decoded.free()
    }
}

import { feed_collateral_mint_offset, feed_lend_mint_offset } from '@calma/wasm-lib'
import type { GetProgramAccountsFilter } from '@solana/web3.js'
import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { connection, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'

/**
 * Byte offsets of the mint fields inside a `Feed` account, measured from the
 * start of the account data (discriminator included), for an RPC `memcmp`.
 *
 * Read from Rust so they cannot fall behind the layout. The hand-written table
 * that used to live here still described the pre-`PriceFeedHeader` struct — the
 * numbers happened to survive the restructure, which is exactly why nobody
 * noticed the comment had stopped being true.
 */
const COLLATERAL_MINT_OFFSET = feed_collateral_mint_offset()
const LEND_MINT_OFFSET = feed_lend_mint_offset()

/** A decoded `Feed` account paired with its on-chain address. */
export interface FeedByPair {
    publicKey: PublicKey
    /** Feed authority (owner of the PDA). */
    authority: PublicKey
    /** `'Pyth'` or `'Manual'`. */
    source: string
    collateralMint: PublicKey
    lendMint: PublicKey
    collateralPrice: bigint
    lendPrice: bigint
    lastUpdatedTs: number
    collateralFeedId: number[]
    lendFeedId: number[]
    /** Raw wire bytes (discriminator included) for WASM parsing via `FeedAccount.from_bytes`. */
    rawAccountData: Uint8Array
}

/** Coerce Anchor's BN/number/bigint scalars into a bigint. */
function toBigInt(v: unknown): bigint {
    if (typeof v === 'bigint') return v
    if (typeof v === 'number') return BigInt(v)
    // BN and similar expose toString()
    return BigInt((v as { toString(): string }).toString())
}

/** Extract the variant key from an Anchor enum object (`{ pyth: {} }`), Capitalized. */
function enumVariant(source: unknown): string {
    const key = Object.keys(source as Record<string, unknown>)[0]
    if (!key) return 'Unknown'
    return key.charAt(0).toUpperCase() + key.slice(1)
}

async function fetchFeedsByPair(
    collateralMint: PublicKey,
    lendMint: PublicKey,
): Promise<FeedByPair[]> {
    // Discriminator memcmp (offset 0) restricts to Feed accounts; the two mint
    // memcmps let the RPC node do the pair matching server-side. The coder
    // camelCases account names, so the Feed account is keyed as `feed`.
    const filters: GetProgramAccountsFilter[] = [
        { memcmp: feedProgram.coder.accounts.memcmp('feed') },
        { memcmp: { offset: COLLATERAL_MINT_OFFSET, bytes: collateralMint.toBase58() } },
        { memcmp: { offset: LEND_MINT_OFFSET, bytes: lendMint.toBase58() } },
    ]

    const accounts = await connection.getProgramAccounts(feedProgram.programId, {
        filters,
    })

    return accounts.map(({ pubkey, account }) => {
        // The coder decodes struct fields as camelCase (matching the generated
        // types), even though the on-chain layout / IDL is snake_case.
        const decoded = feedProgram.coder.accounts.decode('feed', account.data) as Record<string, any>
        const config = decoded.config as Record<string, any>
        const state = decoded.state as Record<string, any>
        return {
            publicKey: pubkey,
            authority: new PublicKey(config.authority),
            source: enumVariant(config.source),
            collateralMint: new PublicKey(decoded.collateralMint),
            lendMint: new PublicKey(decoded.lendMint),
            collateralPrice: toBigInt(state.collateralPrice),
            lendPrice: toBigInt(state.lendPrice),
            lastUpdatedTs: Number(toBigInt(state.lastUpdatedTs)),
            collateralFeedId: config.collateralFeedId as number[],
            lendFeedId: config.lendFeedId as number[],
            rawAccountData: new Uint8Array(account.data),
        }
    })
}

/**
 * Fetch every `Feed` account declared for a given (collateral, lend) mint pair.
 *
 * Uses RPC `memcmp` filters on the two mint fields so the matching happens on
 * the node — no client-side scan of every feed. Disabled until both mints are
 * provided.
 */
export function useFeedsByPair(
    collateralMint: PublicKey | null,
    lendMint: PublicKey | null,
) {
    return useQuery({
        queryKey:
            collateralMint && lendMint
                ? queryKeys.feeds.byPair(collateralMint.toBase58(), lendMint.toBase58())
                : ['feeds', 'pair', 'null', 'null'],
        queryFn: () => fetchFeedsByPair(collateralMint!, lendMint!),
        enabled: !!collateralMint && !!lendMint,
    })
}

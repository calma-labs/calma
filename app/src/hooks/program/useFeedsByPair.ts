import type { GetProgramAccountsFilter } from '@solana/web3.js'
import { PublicKey } from '@solana/web3.js'
import { useQuery } from '@tanstack/react-query'
import { connection, feedProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'

/**
 * Byte offsets of the mint fields inside a `Feed` account, measured from the
 * start of the account data (i.e. including the 8-byte Anchor discriminator).
 *
 * Layout (see `crates/feed-state/src/lib.rs`):
 *   0    discriminator            [8]
 *   8    collateral_mint          [32]  ← memcmp target
 *   40   lend_mint                [32]  ← memcmp target
 *   72   id                       [1]
 *   73   state                    [24]  (collateral_price u64, lend_price u64, last_updated_ts i64)
 *   97   config.authority         [32]
 *   129  config.source            [1]
 *   130  config.bump              [1]
 *   131  config._pad              [6]
 *   137  config.collateral_feed_id[32]
 *   169  config.lend_feed_id      [32]
 *   201  data.collateral_decimals [1]
 *   202  data.lend_decimals       [1]
 *   203  rules                    [30]
 */
const COLLATERAL_MINT_OFFSET = 8
const LEND_MINT_OFFSET = 40

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

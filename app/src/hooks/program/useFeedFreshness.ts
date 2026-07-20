import { HermesClient } from '@pythnetwork/hermes-client'
import { FeedAccount, FeedFreshnessResult, PoolAccount } from '@jbl/wasm-lib'
import { PublicKey } from '@solana/web3.js'
import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { bytesToFeedIdHex, hermesId } from '../../config/pythFeeds'
import { connection } from '../../lib/program'
import { useNow } from '../useNow'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'
const hermes = new HermesClient(HERMES_ENDPOINT)

const REFETCH_INTERVAL_MS = 1_000

/** On-chain scale for `feed.state.{collateral,lend}_price` — see PRICE_SCALE in feed-state. */
const PRICE_SCALE = 1_000_000

export interface OraclePriceSide {
    /** USD price (or `null` if unavailable). */
    price: number | null
    /** Raw Unix timestamp (seconds) of the underlying publish/update. `null` if unavailable.
     *  Compute live age as `now - publishTs` rather than storing a snapshot. */
    publishTs: number | null
    stale: boolean
}

/** Snapshot last written to the feed account by `set_from_pyth`. Both sides
 *  share `feed.last_updated_ts`; the borrow gate uses `pool.max_feed_age_secs`. */
export interface OnChainPrices {
    collateral: OraclePriceSide
    lend: OraclePriceSide
    maxAgeSecs: number
}

/** Latest Pyth publish per side from Hermes. Ages differ per side; the
 *  `set_from_pyth` gate uses `feed.rules.max_age_ms`. `null` when the feed
 *  source isn't pull-Pyth (Manual / PythPush aren't refreshed by the app). */
export interface HermesPrices {
    collateral: OraclePriceSide
    lend: OraclePriceSide
    maxAgeSecs: number
}

export interface FeedFreshness {
    /**
     * The action would fail on-chain: the pool's `StaleOracle` gate rejects
     * the current feed snapshot AND we can't post a fresh Hermes update to
     * fix it. This is what actually disables the borrow / withdraw button.
     */
    willFail: boolean
    onChain: OnChainPrices
    hermes: HermesPrices | null
    reason: string
}

// ── raw snapshot fetched every 20 s ───────────────────────────────────────────

/** Raw bytes + Hermes prices from the network fetch. Staleness is recomputed
 *  every second from this data rather than frozen at fetch time. */
interface FeedSnapshot {
    poolBytes: Uint8Array
    feedBytes: Uint8Array
    /** Hermes `publish_time` for collateral (seconds). 0 = unavailable. */
    hermesCollTs: number
    /** Hermes `publish_time` for lend (seconds). 0 = unavailable. */
    hermesLendTs: number
    hermesCollPrice: number | null
    hermesLendPrice: number | null
}

async function fetchSnapshot(pool: PublicKey): Promise<FeedSnapshot | null> {
    const poolInfo = await connection.getAccountInfo(pool)
    if (!poolInfo) return null
    const poolBytes = new Uint8Array(poolInfo.data)

    const decodedPool = PoolAccount.from_bytes(poolBytes)
    if (!decodedPool) return null
    const feedPubkey = new PublicKey(decodedPool.feed_state)
    decodedPool.free()

    const feedInfo = await connection.getAccountInfo(feedPubkey)
    if (!feedInfo) return null
    const feedBytes = new Uint8Array(feedInfo.data)

    // Parse source + feed IDs to decide whether to call Hermes.
    const decodedFeed = FeedAccount.from_bytes(feedBytes)
    if (!decodedFeed) return null
    const source = decodedFeed.source
    const collFeedId = Array.from(decodedFeed.collateral_feed_id)
    const lendFeedId = Array.from(decodedFeed.lend_feed_id)
    decodedFeed.free()

    if (source !== 1 /* PriceSource::Pyth */) {
        return { poolBytes, feedBytes, hermesCollTs: 0, hermesLendTs: 0, hermesCollPrice: null, hermesLendPrice: null }
    }

    // Always fetch Hermes for pull-Pyth feeds so per-second staleness checks
    // have current publish_time data even when the snapshot is still fresh.
    const collHex = bytesToFeedIdHex(collFeedId)
    const lendHex = bytesToFeedIdHex(lendFeedId)
    const res = await hermes.getLatestPriceUpdates(
        [hermesId(collHex), hermesId(lendHex)],
        { encoding: 'base64' },
    )
    const collParsed = res.parsed?.find((p) => hermesId(p.id) === hermesId(collHex))
    const lendParsed = res.parsed?.find((p) => hermesId(p.id) === hermesId(lendHex))

    return {
        poolBytes,
        feedBytes,
        hermesCollTs: collParsed?.price.publish_time ?? 0,
        hermesLendTs: lendParsed?.price.publish_time ?? 0,
        hermesCollPrice: collParsed
            ? Number(collParsed.price.price) * 10 ** collParsed.price.expo
            : null,
        hermesLendPrice: lendParsed
            ? Number(lendParsed.price.price) * 10 ** lendParsed.price.expo
            : null,
    }
}

// ── per-second staleness computation ─────────────────────────────────────────

function computeFreshness(snapshot: FeedSnapshot, nowSecs: number): FeedFreshness {
    const pool = PoolAccount.from_bytes(snapshot.poolBytes)
    const feed = FeedAccount.from_bytes(snapshot.feedBytes)
    if (!pool || !feed) return { willFail: false, onChain: { collateral: { price: null, publishTs: null, stale: false }, lend: { price: null, publishTs: null, stale: false }, maxAgeSecs: 0 }, hermes: null, reason: '' }

    const now = BigInt(nowSecs)
    const result = FeedFreshnessResult.check(
        pool, feed,
        BigInt(snapshot.hermesCollTs),
        BigInt(snapshot.hermesLendTs),
        now,
    )

    // Read all wasm fields before freeing.
    const willFail = result.will_fail
    const snapshotStale = result.snapshot_stale
    const poolMaxAgeSecs = result.pool_max_age_secs
    const feedMaxAgeSecs = result.feed_max_age_secs
    const hermesCollStale = result.hermes_coll_stale
    const hermesLendStale = result.hermes_lend_stale
    const collPrice = Number(feed.collateral_price) / PRICE_SCALE
    const lendPrice = Number(feed.lend_price) / PRICE_SCALE
    const snapshotTs = Number(feed.last_updated_ts)
    const source = feed.source
    result.free(); pool.free(); feed.free()

    const onChain: OnChainPrices = {
        collateral: { price: collPrice, publishTs: snapshotTs, stale: snapshotStale },
        lend: { price: lendPrice, publishTs: snapshotTs, stale: snapshotStale },
        maxAgeSecs: poolMaxAgeSecs,
    }

    const hermes: HermesPrices | null = source === 1 /* Pyth */ ? {
        collateral: {
            price: snapshot.hermesCollPrice,
            publishTs: snapshot.hermesCollTs || null,
            stale: hermesCollStale,
        },
        lend: {
            price: snapshot.hermesLendPrice,
            publishTs: snapshot.hermesLendTs || null,
            stale: hermesLendStale,
        },
        maxAgeSecs: feedMaxAgeSecs,
    } : null

    let reason = ''
    if (willFail) {
        if (source !== 1) {
            reason = `On-chain snapshot is stale (pool max ${poolMaxAgeSecs}s) and this feed source isn't refreshed by the app.`
        } else {
            const sides = [
                hermesCollStale && 'collateral',
                hermesLendStale && 'lend',
            ].filter(Boolean).join(' and ')
            reason =
                `On-chain snapshot is stale (pool max ${poolMaxAgeSecs}s) ` +
                `and a refresh would be rejected: ${sides} price too old ` +
                `(feed max ${feedMaxAgeSecs}s). Waiting for Pyth to publish.`
        }
    }

    return { willFail, onChain, hermes, reason }
}

// ── public hook ───────────────────────────────────────────────────────────────

/**
 * Predicts whether a borrow / withdraw against this pool would revert with
 * `StaleOracle`. Network data (accounts + Hermes) refetches every 20 s; the
 * staleness verdict and displayed age recompute every second via `useNow`.
 * The button disables only on `willFail`.
 */
export function useFeedFreshness(pool: PublicKey | null | undefined) {
    const { data: snapshot } = useQuery({
        queryKey: ['feed-snapshot', pool?.toBase58() ?? ''],
        queryFn: () => fetchSnapshot(pool!),
        enabled: !!pool,
        refetchInterval: REFETCH_INTERVAL_MS,
        staleTime: REFETCH_INTERVAL_MS / 2,
    })

    const now = useNow()

    const data = useMemo(
        () => (snapshot ? computeFreshness(snapshot, now) : undefined),
        [snapshot, now],
    )

    return { data }
}

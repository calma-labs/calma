import { HermesClient } from '@pythnetwork/hermes-client'
import { PythSolanaReceiver } from '@pythnetwork/pyth-solana-receiver'
import { FeedAccount, PoolAccount } from '@jbl/wasm-lib'
import {
    Connection,
    Keypair,
    PublicKey,
    Transaction,
    VersionedTransaction,
} from '@solana/web3.js'
import { bytesToFeedIdHex, hermesId } from '../../config/pythFeeds'
import { feedProgram } from '../../lib/program'
import { MINTER_KEYPAIR } from '../../store/wallet.store'

const HERMES_ENDPOINT =
    import.meta.env.VITE_HERMES_URL ?? 'https://hermes.pyth.network'

const hermes = new HermesClient(HERMES_ENDPOINT)

/**
 * Wall-clock slack (seconds) between calling `is_pyth_price_stale` and the
 * `set_from_pyth` tx actually landing. Covers Hermes fetch → tx build →
 * `sendAll` confirmation. Predicting against `now + margin` prevents burning
 * two txs on a price that will trip `PriceTooOld` on-chain.
 *
 * MUST stay well under the feed's `max_age`: the gate is `elapsed > max_age`,
 * so a margin ≥ max_age makes the preflight unsatisfiable for *every* Pyth
 * update (even one published this instant lands `margin`s old). Solana
 * confirmation is a couple seconds, so 2s is realistic; the call site further
 * clamps it to `max_age − 1` as a hard guard for tight feeds.
 */
export const PYTH_REFRESH_LATENCY_MARGIN_SECS = 2

/**
 * True when the RPC endpoint is a devnet cluster. Deny-lists mainnet substrings
 * as defense-in-depth: `MINTER_KEYPAIR` is a devnet-only mock authority and
 * must never sign against mainnet even if `VITE_SOLANA_RPC_URL` is misset.
 */
function isDevnetCluster(connection: Connection): boolean {
    const endpoint = connection.rpcEndpoint.toLowerCase()
    if (endpoint.includes('mainnet')) return false
    return endpoint.includes('devnet')
}

/**
 * Refresh any `Pyth`-source feed directly given its on-chain account and feed IDs,
 * signed by MINTER_KEYPAIR. Mirrors the manual 3-step UI flow: post collateral
 * price, post lend price, then commit set_from_pyth against those fresh accounts.
 * Returns all transaction signatures produced across the 3 steps.
 */
async function getTxLogs(connection: Connection, sig: string): Promise<string[]> {
    try {
        const tx = await connection.getTransaction(sig, {
            commitment: 'confirmed',
            maxSupportedTransactionVersion: 0,
        })
        return tx?.meta?.logMessages ?? []
    } catch {
        return [`(failed to fetch logs for ${sig})`]
    }
}

export async function refreshFeedAccount(
    connection: Connection,
    feedPubkey: PublicKey,
    collateralFeedId: number[],
    lendFeedId: number[],
): Promise<string[]> {
    const collHex = bytesToFeedIdHex(collateralFeedId)
    const lendHex = bytesToFeedIdHex(lendFeedId)
    const now = () => Math.floor(Date.now() / 1000)

    const log: object[] = []
    const allSigs: string[] = []

    log.push({ checkpoint: 'start', feed: feedPubkey.toBase58(), collHex, lendHex, wallClock: now() })

    // Read max_age_ms from the feed rules so we can validate Hermes prices
    // before spending two transactions posting a VAA that will be rejected.
    const feedInfo = await connection.getAccountInfo(feedPubkey)
    const maxPythAgeSecs: number = feedInfo
        ? ((feedProgram.coder.accounts.decode('feed', feedInfo.data) as {
              rules: { maxAgeMs: number }
          }).rules.maxAgeMs ?? 60_000) / 1000
        : 60
    log.push({ checkpoint: 'feed_config', maxPythAgeSecs })

    const receiver = new PythSolanaReceiver({
        connection,
        wallet: makeKeeperAnchorWallet(MINTER_KEYPAIR),
    })

    // Step 1: post collateral price update
    const collRes = await hermes.getLatestPriceUpdates([hermesId(collHex)], { encoding: 'base64' })
    if (!collRes.binary.data?.length) throw new Error('Hermes returned no collateral price data')
    const collPublishTime = collRes.parsed?.[0]?.price?.publish_time ?? null
    const collAgeAtFetch = collPublishTime !== null ? now() - collPublishTime : null
    log.push({
        checkpoint: 'step1_hermes',
        publishTime: collPublishTime,
        ageSeconds: collAgeAtFetch,
        wallClock: now(),
    })
    if (collAgeAtFetch !== null && collAgeAtFetch > maxPythAgeSecs) {
        console.error('[refreshFeedAccount]', JSON.stringify(log, null, 2))
        throw new Error(
            `Collateral price from Hermes is ${collAgeAtFetch}s old (max ${maxPythAgeSecs}s). ` +
            `Feed ID ${collHex} is not actively published by Pyth — ` +
            `recreate the feed with a different collateral feed ID.`,
        )
    }

    const collBuilt = await receiver.buildPostPriceUpdateInstructions(collRes.binary.data)
    const collAccount = collBuilt.priceFeedIdToPriceUpdateAccount[hermesId(collHex)]
    if (!collAccount) throw new Error('Receiver did not return a collateral update account')
    const collTxs = await receiver.batchIntoVersionedTransactions(collBuilt.postInstructions, {
        computeUnitPriceMicroLamports: 50_000,
    })
    log.push({ checkpoint: 'step1_sending', collAccount: collAccount.toBase58(), versionedTxCount: collTxs.length, wallClock: now() })

    const collSigs = await receiver.provider.sendAll!(collTxs)
    const collTxLogs = await Promise.all(collSigs.map((s) => getTxLogs(connection, s)))
    log.push({ checkpoint: 'step1_confirmed', sigs: collSigs, txLogs: collTxLogs, wallClock: now() })
    allSigs.push(...collSigs)

    // Step 2: post lend price update
    const lendRes = await hermes.getLatestPriceUpdates([hermesId(lendHex)], { encoding: 'base64' })
    if (!lendRes.binary.data?.length) throw new Error('Hermes returned no lend price data')
    const lendPublishTime = lendRes.parsed?.[0]?.price?.publish_time ?? null
    const lendAgeAtFetch = lendPublishTime !== null ? now() - lendPublishTime : null
    log.push({
        checkpoint: 'step2_hermes',
        publishTime: lendPublishTime,
        ageSeconds: lendAgeAtFetch,
        wallClock: now(),
    })
    if (lendAgeAtFetch !== null && lendAgeAtFetch > maxPythAgeSecs) {
        console.error('[refreshFeedAccount]', JSON.stringify(log, null, 2))
        throw new Error(
            `Lend price from Hermes is ${lendAgeAtFetch}s old (max ${maxPythAgeSecs}s). ` +
            `Feed ID ${lendHex} is not actively published by Pyth — ` +
            `recreate the feed with a different lend feed ID.`,
        )
    }

    const lendBuilt = await receiver.buildPostPriceUpdateInstructions(lendRes.binary.data)
    const lendAccount = lendBuilt.priceFeedIdToPriceUpdateAccount[hermesId(lendHex)]
    if (!lendAccount) throw new Error('Receiver did not return a lend update account')
    const lendTxs = await receiver.batchIntoVersionedTransactions(lendBuilt.postInstructions, {
        computeUnitPriceMicroLamports: 50_000,
    })
    log.push({ checkpoint: 'step2_sending', lendAccount: lendAccount.toBase58(), versionedTxCount: lendTxs.length, wallClock: now() })

    const lendSigs = await receiver.provider.sendAll!(lendTxs)
    const lendTxLogs = await Promise.all(lendSigs.map((s) => getTxLogs(connection, s)))
    log.push({ checkpoint: 'step2_confirmed', sigs: lendSigs, txLogs: lendTxLogs, wallClock: now() })
    allSigs.push(...lendSigs)

    // Step 3: set_from_pyth + close
    log.push({
        checkpoint: 'step3_building',
        collAccount: collAccount.toBase58(),
        lendAccount: lendAccount.toBase58(),
        collAgeSeconds: collPublishTime !== null ? now() - collPublishTime : null,
        lendAgeSeconds: lendPublishTime !== null ? now() - lendPublishTime : null,
        wallClock: now(),
    })

    const setFromPythIx = await feedProgram.methods
        .setFromPyth()
        .accountsPartial({ feed: feedPubkey, collateralPriceUpdate: collAccount, lendPriceUpdate: lendAccount })
        .instruction()
    const closeCollateral = await receiver.buildClosePriceUpdateInstruction(collAccount)
    const closeLend = await receiver.buildClosePriceUpdateInstruction(lendAccount)
    const commitTxs = await receiver.batchIntoVersionedTransactions(
        [{ instruction: setFromPythIx, signers: [] }, closeCollateral, closeLend],
        { computeUnitPriceMicroLamports: 50_000 },
    )
    log.push({ checkpoint: 'step3_sending', versionedTxCount: commitTxs.length, wallClock: now() })

    try {
        const commitSigs = await receiver.provider.sendAll!(commitTxs)
        const commitTxLogs = await Promise.all(commitSigs.map((s) => getTxLogs(connection, s)))
        log.push({ checkpoint: 'step3_confirmed', sigs: commitSigs, txLogs: commitTxLogs, wallClock: now() })
        allSigs.push(...commitSigs)
    } catch (err) {
        log.push({ checkpoint: 'step3_error', error: String(err), wallClock: now() })
        console.error('[refreshFeedAccount]', JSON.stringify(log, null, 2))
        throw err
    }

    console.log('[refreshFeedAccount]', JSON.stringify(log, null, 2))
    return allSigs
}

/**
 * Before a jbl action that reads the feed (borrow / withdrawCollateral / …),
 * post a fresh Pyth **pull** oracle update to the pool's feed PDA, signed and
 * paid by the shipped `MINTER_KEYPAIR`. Devnet only — on mainnet the sponsored
 * push crank keeps feeds fresh and users pay for their own pulls.
 *
 * No-op if:
 *   - The connection isn't devnet.
 *   - The pool's feed isn't `Pyth` source (Manual doesn't need auto-refresh;
 *     PythPush stores pubkeys in the `feed_id` slots, so a pull refresh would
 *     silently target the wrong Hermes feed).
 *
 * Throws on Hermes / send failures; the caller mutation surfaces the error.
 */
export async function refreshFeedForDevnet(
    connection: Connection,
    pool: PublicKey,
): Promise<void> {
    if (!isDevnetCluster(connection)) return

    const poolInfo = await connection.getAccountInfo(pool)
    if (!poolInfo) return
    const decodedPool = PoolAccount.from_bytes(poolInfo.data)
    if (!decodedPool) return
    const feedAccount = new PublicKey(decodedPool.feed_state)

    const feedInfo = await connection.getAccountInfo(feedAccount)
    if (!feedInfo) return
    const feedAcct = FeedAccount.from_bytes(feedInfo.data)
    if (!feedAcct || feedAcct.source !== 1) return // 1 = PriceSource::Pyth (pull)

    // Skip when the on-chain snapshot is still fresh enough for the pool's
    // own `StaleOracle` gate — the caller's action will pass regardless of
    // whether Hermes currently has a fresh update. This mirrors the UI's
    // `useFeedFreshness`: refresh freshness ≠ snapshot freshness.
    const nowSecs = BigInt(Math.floor(Date.now() / 1000))
    if (!decodedPool.is_feed_snapshot_stale(BigInt(feedAcct.last_updated_ts), nowSecs)) {
        return
    }

    const collHex = bytesToFeedIdHex(Array.from(feedAcct.collateral_feed_id))
    const lendHex = bytesToFeedIdHex(Array.from(feedAcct.lend_feed_id))

    const res = await hermes.getLatestPriceUpdates(
        [hermesId(collHex), hermesId(lendHex)],
        { encoding: 'base64' },
    )
    const updateData = res.binary.data
    if (!updateData?.length) throw new Error('Hermes returned no price update data')

    // Ask the wasm binding (single source of truth for the on-chain gate)
    // whether the price will still be fresh at expected tx-landing time. If
    // stale, abort now — posting the VAA would succeed but `set_from_pyth`
    // would revert with `PriceTooOld` (0x3e80), wasting two txs.
    // Clamp the latency margin to `max_age − 1s`: with `elapsed > max_age` as
    // the gate, a margin ≥ max_age would reject every possible update. `0`
    // disables the on-chain check, so no clamp is needed there.
    const maxAgeSecs = feedAcct.max_age_ms / 1000
    const nowTs = Math.floor(Date.now() / 1000)
    const marginSecs =
        maxAgeSecs > 0
            ? Math.min(PYTH_REFRESH_LATENCY_MARGIN_SECS, maxAgeSecs - 1)
            : PYTH_REFRESH_LATENCY_MARGIN_SECS
    const expectedClockTs = BigInt(nowTs + marginSecs)
    for (const [label, hex] of [['collateral', collHex], ['lend', lendHex]] as const) {
        const parsed = res.parsed?.find((p) => hermesId(p.id) === hermesId(hex))
        const publishTime = parsed?.price?.publish_time
        if (publishTime == null) continue
        if (feedAcct.is_pyth_price_stale(BigInt(publishTime), expectedClockTs)) {
            // Report the age the gate actually judged (at expected landing),
            // not the wall-clock age now — otherwise a "2s old" message looks
            // fresh while the gate rejected it against `now + margin`.
            const ageAtLandingSecs = nowTs + marginSecs - publishTime
            throw new Error(
                `${label} Pyth price will be ${ageAtLandingSecs}s old at tx ` +
                `landing (feed max ${maxAgeSecs}s, incl. ${marginSecs}s latency ` +
                `margin). Waiting for Pyth to publish a fresher update for feed ` +
                `ID ${hex.slice(0, 8)}…`,
            )
        }
    }

    const receiver = new PythSolanaReceiver({
        connection,
        wallet: makeKeeperAnchorWallet(MINTER_KEYPAIR),
    })

    const builder = receiver.newTransactionBuilder({ closeUpdateAccounts: true })
    await builder.addPostPriceUpdates(updateData)
    await builder.addPriceConsumerInstructions(async (getPriceUpdateAccount) => {
        const ix = await feedProgram.methods
            .setFromPyth()
            .accountsPartial({
                feed: feedAccount,
                collateralPriceUpdate: getPriceUpdateAccount(hermesId(collHex)),
                lendPriceUpdate: getPriceUpdateAccount(hermesId(lendHex)),
            })
            .instruction()
        return [{ instruction: ix, signers: [] }]
    })

    const txs = await builder.buildVersionedTransactions({
        computeUnitPriceMicroLamports: 50_000,
    })
    await receiver.provider.sendAll(txs)
}

/**
 * Anchor-wallet shim backed by a raw `Keypair`. Same shape as the private
 * `makeAnchorWallet` in `usePullPythFeed.ts`, but sign uses the keypair's
 * secret key directly rather than delegating to a wallet session.
 */
function makeKeeperAnchorWallet(keypair: Keypair) {
    const wallet = {
        publicKey: keypair.publicKey,
        signTransaction: async <T extends Transaction | VersionedTransaction>(tx: T): Promise<T> => {
            if (tx instanceof VersionedTransaction) {
                tx.sign([keypair])
            } else {
                tx.partialSign(keypair)
            }
            return tx
        },
        signAllTransactions: async <T extends Transaction | VersionedTransaction>(txs: T[]): Promise<T[]> => {
            for (const tx of txs) {
                if (tx instanceof VersionedTransaction) tx.sign([keypair])
                else tx.partialSign(keypair)
            }
            return txs
        },
    }
    return wallet as unknown as ConstructorParameters<typeof PythSolanaReceiver>[0]['wallet']
}

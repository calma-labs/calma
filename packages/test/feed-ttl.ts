import * as anchor from "@anchor-lang/core";
import { expect } from "chai";
import { setupTest, participateInPool, setFeedPrice, setFeedTtl, TestSetup } from "./utils";

/**
 * End-to-end coverage for the oracle-owned staleness budget.
 *
 * The pool used to carry its own `max_feed_age_ms`; the budget now lives on the
 * feed as `price_ttl_ms`, because the feed is the only party that knows its own
 * update cadence. These tests drive the gate through a real validator: retune
 * the feed, let wall-clock time pass, and watch the borrow flip.
 */

const LEND_LIQUIDITY = 500_000_000;
const COLLATERAL_DEPOSIT = 100_000_000;
const BORROW_AMOUNT = 10_000_000;

/** Short enough to lapse inside a test, long enough to survive tx confirmation. */
const SHORT_TTL_MS = 1_000;
/**
 * Wall-clock wait used to age the price past `SHORT_TTL_MS`.
 *
 * Deliberately generous: a validator's `Clock::unix_timestamp` is a slot-based
 * estimate that drifts against wall clock, so a wait only slightly longer than
 * the TTL is a flaky test. The *exact* boundary is pinned deterministically in
 * `programs/calma/tests/test_oracle_interface.rs`, where the clock is settable;
 * what this file proves is that the wiring works on a real validator.
 */
const LAPSE_WAIT_MS = 6_000;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function depositCollateral(setup: TestSetup, amount: number) {
    await setup.program.methods
        .depositCollateral(new anchor.BN(amount))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            collateralMint: setup.collateralMint,
            authority: setup.authority.publicKey,
            userTokenAccount: setup.userCollateralTokenAccount,
        })
        .signers([setup.authority])
        .rpc();
}

async function borrow(setup: TestSetup, amount: number) {
    await setup.program.methods
        .borrow(new anchor.BN(amount))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: setup.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
            feedState: setup.feedPda,
        })
        .signers([setup.authority])
        .rpc();
}

async function expectStaleOracle(setup: TestSetup) {
    try {
        await borrow(setup, BORROW_AMOUNT);
        expect.fail("expected borrow to be rejected as StaleOracle");
    } catch (e: any) {
        const msg = e.message as string;
        if (msg.includes("expected") && msg.includes("to be rejected")) throw e;
        expect(msg).to.include("StaleOracle");
    }
}

describe("feed-owned price TTL", () => {
    describe("a lapsed TTL blocks borrowing until the price is rewritten", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, COLLATERAL_DEPOSIT);
        });

        it("borrows fine under the default budget", async () => {
            await borrow(setup, BORROW_AMOUNT);

            // `gte`, not `equal`: interest accrues between instructions.
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.gte(new anchor.BN(BORROW_AMOUNT))).to.be.true;
        });

        it("rejects the borrow once the feed's own TTL has lapsed", async () => {
            // Narrowing the budget alone is enough — the price itself is untouched,
            // and no pool-side configuration exists to change any more.
            await setFeedTtl(setup, SHORT_TTL_MS);
            await sleep(LAPSE_WAIT_MS);

            await expectStaleOracle(setup);
        });

        it("accepts it again after widening the budget, with no price rewrite", async () => {
            // Same stale price, wider budget. Proves the gate reads `price_ttl_ms`
            // live rather than having baked a deadline in when the price was
            // written — and that the feed alone decides, since the pool was never
            // touched across the flip.
            await setFeedTtl(setup, 3_600_000);

            await borrow(setup, BORROW_AMOUNT);
        });

        it("goes stale again the moment the budget is narrowed back", async () => {
            // No new wait needed: the price is already well past SHORT_TTL_MS old.
            await setFeedTtl(setup, SHORT_TTL_MS);

            await expectStaleOracle(setup);
        });

        it("accepts it again as soon as the feed rewrites the price", async () => {
            // The other way out of staleness: leave the tight budget in place and
            // refresh the price instead.
            await setFeedPrice(setup, 1_000_000, 1_000_000);

            await borrow(setup, BORROW_AMOUNT);

            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.gte(new anchor.BN(BORROW_AMOUNT * 3))).to.be.true;
        });
    });

    describe("zero is refused rather than treated as 'no limit'", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("rejects setPriceTtl(0)", async () => {
            try {
                await setFeedTtl(setup, 0);
                expect.fail("expected setPriceTtl(0) to be rejected");
            } catch (e: any) {
                const msg = e.message as string;
                if (msg.includes("expected") && msg.includes("to be rejected")) throw e;
                expect(msg).to.include("InvalidPriceTtl");
            }
        });
    });
});

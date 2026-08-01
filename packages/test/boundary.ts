import { BN } from "@anchor-lang/core";
import { mintTo } from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, participateInPool, setFeedPrice, TestSetup } from "./utils";

// ── Shared helpers (mirror oracle-price.ts pattern) ───────────────────────────

async function depositCollateral(setup: TestSetup, amount: BN | number) {
    await setup.program.methods
        .depositCollateral(new BN(amount.toString()))
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

async function borrow(setup: TestSetup, amount: BN | number) {
    await setup.program.methods
        .borrow(new BN(amount.toString()))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: setup.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
            feedProgram: setup.feedProgram.programId,
            feedState: setup.feedPda,
        })
        .signers([setup.authority])
        .rpc();
}

// Repay does not read the oracle — no feedProgram / feedState accounts.
async function repay(setup: TestSetup, amount: BN | number) {
    await setup.program.methods
        .repay(new BN(amount.toString()))
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: setup.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
        })
        .signers([setup.authority])
        .rpc();
}

// depositLent with BN — bypasses the number-only participateInPool helper for large amounts.
async function depositLent(setup: TestSetup, amount: BN) {
    await setup.program.methods
        .depositLent(amount)
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: setup.authority.publicKey,
            userLendTokenAccount: setup.userLendTokenAccount,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
        })
        .signers([setup.authority])
        .rpc();
}

async function expectRejected(label: string, fn: () => Promise<void>) {
    try {
        await fn();
        expect.fail(`expected ${label} to be rejected`);
    } catch (e: any) {
        if ((e.message as string).includes("expected") && (e.message as string).includes("to be rejected")) throw e;
        expect(e.message).to.include("Undercollateralized");
    }
}

// ── 1. Minimal amounts ────────────────────────────────────────────────────────

describe("boundary: minimal amounts", () => {
    // 133 base units of collateral, 75% LTV → floor(133×75/100) = 99 units borrowable.
    // borrow(99) must succeed; borrow(100) must be rejected by the on-chain LTV guard.
    describe("133-unit collateral position — exact floor boundary", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 1_000_000); // tiny vault, enough for 99 units
            await depositCollateral(setup, 133);
        });

        it("borrow(99) succeeds — floor(133×75/100)=99", async () => {
            await borrow(setup, 99);
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.toString()).to.equal("99");
        });

        it("borrow(1 more) is rejected — 100 exceeds 75% LTV of 133", async () => {
            await expectRejected("borrow over LTV", () => borrow(setup, 1));
        });
    });

    // A single base unit of collateral → max_borrowable = 0 → any borrow rejected.
    describe("1-unit collateral — zero borrow capacity", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 1_000_000);
            await depositCollateral(setup, 1);
        });

        it("borrow(1) is rejected — no capacity at 1 base unit", async () => {
            await expectRejected("borrow on 1-unit collateral", () => borrow(setup, 1));
        });
    });
});

// ── 2. 100M token position (10^14 base units) ─────────────────────────────────
//
// 100M tokens × 6 decimals = 1e14 base units. Fits in u64 (max ~1.8e19).
// Exceeds the 1B default mint; we call mintTo again before depositing.

describe("boundary: 100M token position", () => {
    // 100_000_000 tokens × 1_000_000 decimals = 1e14 base units
    const HUNDRED_M_TOKENS = new BN("100000000000000");
    // 75% of 100M tokens = 75M tokens = 7.5e13 base units
    const SEVENTY_FIVE_M_TOKENS = new BN("75000000000000");
    // 80M tokens: 5M extra over capacity keeps the vault non-empty after borrowing max,
    // so the LTV guard (not the vault-balance check) rejects the +1 unit attempt.
    const LEND_LIQUIDITY = new BN("80000000000000");

    let setup: TestSetup;
    before(async () => {
        setup = await setupTest();

        // Mint extra tokens so authority can deposit 100M collateral and 80M lend liquidity.
        await mintTo(
            setup.connection, setup.payer,
            setup.collateralMint, setup.userCollateralTokenAccount,
            setup.authority,
            BigInt(HUNDRED_M_TOKENS.toString())
        );
        await mintTo(
            setup.connection, setup.payer,
            setup.lendMint, setup.userLendTokenAccount,
            setup.authority,
            BigInt(LEND_LIQUIDITY.toString())
        );

        await depositLent(setup, LEND_LIQUIDITY);
        await depositCollateral(setup, HUNDRED_M_TOKENS);
    });

    it("pool records 1e14 collateral units in the vault", async () => {
        const pos = await setup.program.account.userPosition.fetch(setup.userPositionPda);
        expect(pos.collateralDeposited.toString()).to.equal(HUNDRED_M_TOKENS.toString());
    });

    it("borrow(75_000_000_000_000) — exact 75% LTV at 100M scale — succeeds", async () => {
        await borrow(setup, SEVENTY_FIVE_M_TOKENS);
        const pool = await setup.program.account.pool.fetch(setup.pool);
        expect(pool.market.totalBorrowAssets.toString()).to.equal(
            SEVENTY_FIVE_M_TOKENS.toString()
        );
        expect(pool.market.totalBorrowShares.toString()).to.not.equal("0");
    });

    it("borrow(1 more base unit) is rejected — 1 unit over capacity", async () => {
        await expectRejected("borrow over capacity at 100M scale", () => borrow(setup, 1));
    });

    it("full repay zeroes out pool debt", async () => {
        // Overpay (protocol caps to actual debt); both assets and shares must reach zero.
        await repay(setup, HUNDRED_M_TOKENS);
        const pool = await setup.program.account.pool.fetch(setup.pool);
        expect(pool.market.totalBorrowAssets.toString()).to.equal("0");
        expect(pool.market.totalBorrowShares.toString()).to.equal("0");
        const pos = await setup.program.account.userPosition.fetch(setup.userPositionPda);
        expect(pos.debtShares.toString()).to.equal("0");
    });
});

// ── 3. Oracle price boundary values ──────────────────────────────────────────
//
// oracle_price (computed in FeedAccount::price) = collateralPrice × PRICE_SCALE / lendPrice
//   (both tokens have 6 decimals, so decimal powers cancel).
// Borrow capacity = collateral × oracle_price / PRICE_SCALE × LTV% / 100.

describe("boundary: oracle price values", () => {
    const COLLATERAL = 100_000_000; // 100 tokens — within default 1B mint limit

    // ── 3a. 1000:1 — collateral is 1000× more valuable ───────────────────────
    // oracle_price = 1_000_000_000 × PRICE_SCALE / PRICE_SCALE = 1_000_000_000
    // capacity = 100M × 1_000_000_000 / PRICE_SCALE × 75% = 75_000_000_000
    describe("collateral 1000x more valuable (1_000_000_000 : 1_000_000)", () => {
        const CAPACITY = 75_000_000_000;
        const UNDER_CAPACITY = 50_000_000_000;
        // 1B buffer keeps vault non-empty so the LTV guard fires on the +1 borrow.
        const LEND_DEPOSIT = CAPACITY + 1_000_000_000;

        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await mintTo(
                setup.connection, setup.payer,
                setup.lendMint, setup.userLendTokenAccount,
                setup.authority,
                BigInt(LEND_DEPOSIT)
            );
            await depositLent(setup, new BN(LEND_DEPOSIT.toString()));
            await depositCollateral(setup, COLLATERAL);
            await setFeedPrice(setup, 1_000_000_000, 1_000_000);
        });

        it("borrow well under capacity succeeds without overflow", async () => {
            await borrow(setup, UNDER_CAPACITY);
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.toString()).to.equal(UNDER_CAPACITY.toString());
        });

        it("borrow at capacity + 1 is rejected", async () => {
            await repay(setup, new BN(CAPACITY.toString()));
            await expectRejected("borrow over 1000:1 capacity", () => borrow(setup, CAPACITY + 1));
        });
    });

    // ── 3b. 1:1000 — collateral is 1000× cheaper ─────────────────────────────
    // oracle_price = 1_000 × PRICE_SCALE / 1_000_000 = 1_000
    // capacity = 100M × 1_000 / PRICE_SCALE × 75% = 75_000
    describe("collateral 1000x cheaper (1_000 : 1_000_000)", () => {
        const CAPACITY = 75_000;

        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 1_000_000);
            await depositCollateral(setup, COLLATERAL);
            await setFeedPrice(setup, 1_000, 1_000_000);
        });

        it("borrow exactly 75_000 units succeeds", async () => {
            await borrow(setup, CAPACITY);
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.toString()).to.equal(CAPACITY.toString());
        });

        it("borrow(75_001) is rejected — 1 unit over capacity", async () => {
            await repay(setup, CAPACITY * 2);
            await expectRejected("borrow over 1:1000 capacity", () => borrow(setup, CAPACITY + 1));
        });
    });

    // ── 3c. Large equal prices (5_000_000 : 5_000_000 = 1:1) ─────────────────
    // oracle_price = 5_000_000 × PRICE_SCALE / 5_000_000 = PRICE_SCALE (same as 1:1).
    // Verifies that large absolute prices cancel correctly and don't overflow u128.
    // capacity = 100M × 1:1 × 75% = 75M — same as default.
    describe("large equal prices (5_000_000 : 5_000_000)", () => {
        const BORROW = 50_000_000; // well under 75M capacity

        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 500_000_000);
            await depositCollateral(setup, COLLATERAL);
            await setFeedPrice(setup, 5_000_000, 5_000_000);
        });

        it("capacity is the same as 1:1 ratio — borrow 50M succeeds", async () => {
            await borrow(setup, BORROW);
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.toString()).to.equal(BORROW.toString());
        });

        it("over-75M borrow is rejected", async () => {
            await repay(setup, BORROW * 2);
            await expectRejected("borrow over 75M at equal prices", () => borrow(setup, 75_000_001));
        });
    });

    // ── 3d. Oracle rounds to zero (integer division floor) ────────────────────
    // oracle_price = 1 × PRICE_SCALE / 1_000_001 = 1_000_000 / 1_000_001 = 0 (floor).
    // max_borrow_capacity = collateral × 0 / PRICE_SCALE × LTV% / 100 = 0.
    // Any borrow amount is rejected regardless of collateral size.
    describe("oracle price rounds to 0 (collateralPrice=1, lendPrice=1_000_001)", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 1_000_000);
            await depositCollateral(setup, COLLATERAL);
            await setFeedPrice(setup, 1, 1_000_001);
        });

        it("borrow(1) is rejected — oracle ratio floors to zero", async () => {
            await expectRejected("borrow at zero oracle price", () => borrow(setup, 1));
        });
    });

    // ── 3e. Price crash mid-session ───────────────────────────────────────────
    // Open a healthy borrow at 1:1, then crash collateral price so the existing
    // debt exceeds the new capacity. Any additional borrow is rejected.
    describe("price crash after open borrow", () => {
        const INITIAL_BORROW = 50_000_000;

        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, 500_000_000);
            await depositCollateral(setup, COLLATERAL);
            await borrow(setup, INITIAL_BORROW); // healthy: 50M of 75M capacity at 1:1
            // Crash: collateral → 0.1× lend price → new capacity = 100M × 0.1 × 75% = 7.5M.
            await setFeedPrice(setup, 100_000, 1_000_000);
        });

        it("existing debt is preserved — borrow path does not auto-liquidate", async () => {
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.totalBorrowAssets.gte(new BN(INITIAL_BORROW))).to.be.true;
        });

        it("any additional borrow is rejected after the price crash", async () => {
            // Existing 50M debt > new 7.5M capacity, so the LTV guard fires immediately.
            await expectRejected("borrow after price crash", () => borrow(setup, 1));
        });
    });
});

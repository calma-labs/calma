import * as anchor from "@anchor-lang/core";
import { expect } from "chai";
import { setupTest, participateInPool, setFeedPrice, TestSetup } from "./utils";

/** Deposited into the lend vault so borrowers have plenty to borrow against. */
const LEND_LIQUIDITY = 500_000_000; // 500 lend tokens
/** Both tests deposit the same collateral so the only variable is the oracle price. */
const COLLATERAL_DEPOSIT = 100_000_000; // 100 collateral tokens
/** Default pool LTV from setupTest. */
const LTV_PERCENT = 75;

/** Both tests attempt the exact same borrow; only the oracle price differs. */
const BORROW_AMOUNT = 50_000_000; // 50 lend tokens

async function depositCollateral(setup: TestSetup, amount: number) {
    await setup.program.methods
        .depositCollateral(new anchor.BN(amount))
        .accounts({
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

// Baseline prices: both assets at 2.0, so the ratio is 1.0 and the 50M borrow
// sits comfortably under the 75M capacity. Each denial test moves exactly one
// price off this baseline.
const BASE_COLLATERAL_PRICE = 2_000_000; // 2.0
const BASE_LEND_PRICE = 2_000_000; // 2.0

/** Fresh pool with the standard liquidity/collateral and both oracle prices set. */
async function prepare(collateralPrice: number, lendPrice: number): Promise<TestSetup> {
    const setup = await setupTest();
    await participateInPool(setup, LEND_LIQUIDITY);
    await depositCollateral(setup, COLLATERAL_DEPOSIT);
    await setFeedPrice(setup, collateralPrice, lendPrice);
    return setup;
}

async function expectUndercollateralized(setup: TestSetup) {
    try {
        await borrow(setup, BORROW_AMOUNT);
        expect.fail("expected borrow to be rejected as undercollateralized");
    } catch (e: any) {
        const msg = e.message as string;
        if (msg.includes("expected") && msg.includes("to be rejected")) throw e;
        expect(msg).to.include("Undercollateralized");
    }
}

// Borrow capacity is `collateral × (collateralPrice / lendPrice) × LTV%`, so it
// depends on BOTH asset prices. All three tests attempt the same 50M borrow;
// only the oracle prices differ. From an accepted baseline, cheapening the
// collateral OR making the lend (borrowed) asset pricier both drop capacity
// below the borrow — proving each side of the ratio moves capacity.
describe("oracle price scales borrow capacity", () => {
    it("allows the borrow at the baseline ratio", async () => {
        // ratio 2.0/2.0 = 1.0 → capacity = 100M × 1.0 × 75% = 75M, above the borrow.
        const setup = await prepare(BASE_COLLATERAL_PRICE, BASE_LEND_PRICE);

        await borrow(setup, BORROW_AMOUNT);

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);
        expect(poolAccount.market.totalBorrowAssets.toString()).to.equal(
            BORROW_AMOUNT.toString()
        );
    });

    it("rejects the borrow when the collateral gets cheaper", async () => {
        // Collateral halved (1.0), lend unchanged → ratio 0.5 →
        // capacity = 100M × 0.5 × 75% = 37.5M, below the borrow.
        const setup = await prepare(1_000_000, BASE_LEND_PRICE);

        await expectUndercollateralized(setup);
    });

    it("rejects the borrow when the lend asset gets more expensive", async () => {
        // Lend doubled (4.0), collateral unchanged → ratio 0.5 →
        // capacity = 100M × 0.5 × 75% = 37.5M, below the borrow.
        const setup = await prepare(BASE_COLLATERAL_PRICE, 4_000_000);

        await expectUndercollateralized(setup);
    });
});

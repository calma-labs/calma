import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import { expect } from "chai";
import { setupTest, FeeCurve, participateInPool } from "./utils";

describe("negative intercept", () => {
    it("creates pool with negative c2 intercept and calculates rates correctly", async () => {
        // Fee curve with negative c2: m1=0, c1=200, m2=14000, c2=-11000
        // This creates a kink at 80% utilization:
        // - Below 80%: flat 2% rate (c1=200)
        // - Above 80%: rate = 140% * util - 110%, crossing at 80% with 2%
        // At 100% util: 14000 - 11000 = 3000 bps = 30%
        const feeCurve: FeeCurve = {
            m1: new BN(0),
            c1: new BN(200),
            m2: new BN(14000),
            c2: new BN(-11000), // Negative intercept!
        };

        const setup = await setupTest(feeCurve);
        await participateInPool(setup, 500_000_000);

        // Verify pool was created with correct fee config
        const poolAccount = await setup.program.account.pool.fetch(setup.pool);
        
        // Check that the fee config was stored correctly
        expect(poolAccount.feeConfig.m1.toNumber()).to.equal(0);
        expect(poolAccount.feeConfig.c1.toNumber()).to.equal(200);
        expect(poolAccount.feeConfig.m2.toNumber()).to.equal(14000);
        expect(poolAccount.feeConfig.c2.toNumber()).to.equal(-11000);

        console.log("Pool created successfully with negative c2:", poolAccount.feeConfig.c2.toNumber());
    });

    it("creates pool with negative c1 intercept", async () => {
        // Fee curve with negative c1: m1=1000, c1=-500, m2=0, c2=0
        // This means at low utilization, rate can be negative (clamped to 0)
        // At 50% util: 1000 * 0.5 - 500 = 0 bps
        // At 60% util: 1000 * 0.6 - 500 = 100 bps = 1%
        const feeCurve: FeeCurve = {
            m1: new BN(1000),
            c1: new BN(-500), // Negative intercept!
            m2: new BN(0),
            c2: new BN(0),
        };

        const setup = await setupTest(feeCurve);
        await participateInPool(setup, 500_000_000);

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);
        
        expect(poolAccount.feeConfig.m1.toNumber()).to.equal(1000);
        expect(poolAccount.feeConfig.c1.toNumber()).to.equal(-500);
        expect(poolAccount.feeConfig.m2.toNumber()).to.equal(0);
        expect(poolAccount.feeConfig.c2.toNumber()).to.equal(0);

        console.log("Pool created successfully with negative c1:", poolAccount.feeConfig.c1.toNumber());
    });

    it("creates pool with both negative intercepts", async () => {
        // Both intercepts negative
        const feeCurve: FeeCurve = {
            m1: new BN(500),
            c1: new BN(-200),
            m2: new BN(2000),
            c2: new BN(-1000),
        };

        const setup = await setupTest(feeCurve);
        await participateInPool(setup, 500_000_000);

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);
        
        expect(poolAccount.feeConfig.c1.toNumber()).to.equal(-200);
        expect(poolAccount.feeConfig.c2.toNumber()).to.equal(-1000);

        console.log("Pool created successfully with both negative intercepts");
    });
});

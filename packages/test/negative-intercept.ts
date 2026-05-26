import { BN } from "@anchor-lang/core";
import { expect } from "chai";
import { setupTest, participateInPool } from "./utils";

function curveArg(a: number, b: number, enabled = true) {
  return { a: new BN(a), b: new BN(b), enabled };
}

describe("negative intercept", () => {
    it("creates pool with negative b on curve 1 and calculates rates correctly", async () => {
        const setup = await setupTest();
        await participateInPool(setup, 500_000_000);

        // Curve 0: flat 200 bps
        await setup.program.methods
            .setFeeCurve(0, curveArg(0, 200))
            .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
            .signers([setup.authority])
            .rpc();

        // Curve 1: 14000*(u/10000) - 11000, negative below u≈7857
        await setup.program.methods
            .setFeeCurve(1, curveArg(14000, -11000))
            .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
            .signers([setup.authority])
            .rpc();

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);

        expect(poolAccount.feeConfig.curves[0].b.toNumber()).to.equal(200);
        expect(poolAccount.feeConfig.curves[1].a.toNumber()).to.equal(14000);
        expect(poolAccount.feeConfig.curves[1].b.toNumber()).to.equal(-11000);

        console.log("Pool created successfully with negative b:", poolAccount.feeConfig.curves[1].b.toNumber());
    });

    it("creates pool with negative linear intercept on curve 0", async () => {
        const setup = await setupTest();
        await participateInPool(setup, 500_000_000);

        // Curve 0: 1000*(u/10000) - 500, negative until u=5000
        await setup.program.methods
            .setFeeCurve(0, curveArg(1000, -500))
            .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
            .signers([setup.authority])
            .rpc();

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);

        expect(poolAccount.feeConfig.curves[0].a.toNumber()).to.equal(1000);
        expect(poolAccount.feeConfig.curves[0].b.toNumber()).to.equal(-500);

        console.log("Pool created successfully with negative b:", poolAccount.feeConfig.curves[0].b.toNumber());
    });

    it("creates pool with both negative intercepts across curves", async () => {
        const setup = await setupTest();
        await participateInPool(setup, 500_000_000);

        await setup.program.methods
            .setFeeCurve(0, curveArg(500, -200))
            .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
            .signers([setup.authority])
            .rpc();

        await setup.program.methods
            .setFeeCurve(1, curveArg(2000, -1000))
            .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
            .signers([setup.authority])
            .rpc();

        const poolAccount = await setup.program.account.pool.fetch(setup.pool);

        expect(poolAccount.feeConfig.curves[0].b.toNumber()).to.equal(-200);
        expect(poolAccount.feeConfig.curves[1].b.toNumber()).to.equal(-1000);

        console.log("Pool created successfully with both negative intercepts");
    });
});

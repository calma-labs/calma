import { BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import { setupTest, TestSetup } from "./utils";

function curve(a: number, b: number, enabled = true) {
    return { a: new BN(a), b: new BN(b), enabled };
}

describe("fee curve management", () => {
    describe("default state after create", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("curve 0 is enabled with b=100 and a=0", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            const c0 = poolAccount.feeConfig.curves[0];
            expect(c0.a.toNumber()).to.equal(0);
            expect(c0.b.toNumber()).to.equal(100);
            expect(c0.enabled).to.not.equal(0);
        });

        it("curves 1–3 are all-zero and disabled", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            for (const idx of [1, 2, 3]) {
                const c = poolAccount.feeConfig.curves[idx];
                expect(c.a.toNumber()).to.equal(0, `curve ${idx} a`);
                expect(c.b.toNumber()).to.equal(0, `curve ${idx} b`);
                expect(c.enabled).to.equal(0, `curve ${idx} enabled`);
            }
        });
    });

    describe("setFeeCurve — overwrites curve fields", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("writes a, b, enabled to curve 0", async () => {
            await setup.program.methods
                .setFeeCurve(0, curve(200, 50, true))
                .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                .signers([setup.authority])
                .rpc();

            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            const c0 = poolAccount.feeConfig.curves[0];
            expect(c0.a.toNumber()).to.equal(200);
            expect(c0.b.toNumber()).to.equal(50);
            expect(c0.enabled).to.not.equal(0);
        });
    });

    describe("setFeeCurve — writes to any valid index", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        for (const idx of [1, 2, 3] as const) {
            it(`writes to index ${idx}`, async () => {
                await setup.program.methods
                    .setFeeCurve(idx, curve(idx * 100, idx * 10, true))
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();

                const poolAccount = await setup.program.account.pool.fetch(setup.pool);
                expect(poolAccount.feeConfig.curves[idx].a.toNumber()).to.equal(idx * 100);
            });
        }
    });

    describe("setFeeCurve — rejects index ≥ 4", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("throws InvalidCurveIndex for index 4", async () => {
            try {
                await setup.program.methods
                    .setFeeCurve(4, curve(0, 100))
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });

    describe("setFeeCurve — rejects non-authority signer", () => {
        let setup: TestSetup;
        let stranger: Keypair;

        before(async () => {
            setup = await setupTest();
            stranger = Keypair.generate();
            const sig = await setup.connection.requestAirdrop(stranger.publicKey, 0.1 * LAMPORTS_PER_SOL);
            await setup.connection.confirmTransaction(sig);
        });

        it("throws Unauthorized when signed by a random keypair", async () => {
            try {
                await setup.program.methods
                    .setFeeCurve(0, curve(0, 100))
                    .accounts({ pool: setup.pool, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });

    describe("enableFeeCurve / disableFeeCurve — toggles enabled flag", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("disableFeeCurve(0) sets enabled to 0", async () => {
            await setup.program.methods
                .disableFeeCurve(0)
                .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                .signers([setup.authority])
                .rpc();

            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.feeConfig.curves[0].enabled).to.equal(0);
        });

        it("enableFeeCurve(0) sets enabled back to non-zero", async () => {
            await setup.program.methods
                .enableFeeCurve(0)
                .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                .signers([setup.authority])
                .rpc();

            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.feeConfig.curves[0].enabled).to.not.equal(0);
        });
    });

    describe("disableFeeCurve — rejects non-authority signer", () => {
        let setup: TestSetup;
        let stranger: Keypair;

        before(async () => {
            setup = await setupTest();
            stranger = Keypair.generate();
            const sig = await setup.connection.requestAirdrop(stranger.publicKey, 0.1 * LAMPORTS_PER_SOL);
            await setup.connection.confirmTransaction(sig);
        });

        it("throws Unauthorized when signed by a random keypair", async () => {
            try {
                await setup.program.methods
                    .disableFeeCurve(0)
                    .accounts({ pool: setup.pool, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });

    describe("enableFeeCurve — rejects non-authority signer", () => {
        let setup: TestSetup;
        let stranger: Keypair;

        before(async () => {
            setup = await setupTest();
            stranger = Keypair.generate();
            const sig = await setup.connection.requestAirdrop(stranger.publicKey, 0.1 * LAMPORTS_PER_SOL);
            await setup.connection.confirmTransaction(sig);
        });

        it("throws Unauthorized when signed by a random keypair", async () => {
            try {
                await setup.program.methods
                    .enableFeeCurve(0)
                    .accounts({ pool: setup.pool, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });

    describe("enableFeeCurve — rejects index ≥ 4", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("throws InvalidCurveIndex for index 4", async () => {
            try {
                await setup.program.methods
                    .enableFeeCurve(4)
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });

    describe("disableFeeCurve — rejects index ≥ 4", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
        });

        it("throws InvalidCurveIndex for index 4", async () => {
            try {
                await setup.program.methods
                    .disableFeeCurve(4)
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("should have thrown");
            } catch (err) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });
});

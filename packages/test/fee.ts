import { BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import { setupTest, TestSetup } from "./utils";

// Interest accrual (which mints the fee shares) is exercised with clock control
// in the Rust LiteSVM suite (`programs/calma/tests/test_fee.rs`) and the math
// unit tests. Here we cover the on-chain plumbing through the real Anchor stack:
// the authority-only `set_fee`/`claim_fees` entrypoints and their guard paths.
describe("protocol fee", () => {
    describe("set_fee", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
        });

        it("authority sets the fee rate", async () => {
            await setup.program.methods
                .setFee(new BN(2000))
                .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                .signers([setup.authority])
                .rpc();
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.fee.toString()).to.equal("2000");
        });

        it("rejects a fee above MAX_FEE_BPS", async () => {
            try {
                await setup.program.methods
                    .setFee(new BN(2501))
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("over-cap fee should have been rejected");
            } catch (e: any) {
                expect(e.toString()).to.include("FeeTooHigh");
            }
        });

        it("rejects a non-authority caller", async () => {
            const stranger = Keypair.generate();
            const sig = await setup.connection.requestAirdrop(stranger.publicKey, LAMPORTS_PER_SOL);
            await setup.connection.confirmTransaction(sig);
            try {
                await setup.program.methods
                    .setFee(new BN(100))
                    .accounts({ pool: setup.pool, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("non-authority set_fee should have been rejected");
            } catch (e: any) {
                expect(e.toString()).to.include("Unauthorized");
            }
        });
    });

    describe("claim_fees", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
        });

        it("rejects a claim when no fees have accrued", async () => {
            // state, lpMint, the authority ATA, and the programs are all derived
            // by Anchor's account resolver from `pool` + `authority`.
            try {
                await setup.program.methods
                    .claimFees()
                    .accounts({ pool: setup.pool, authority: setup.authority.publicKey })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("claim with no accrued fees should have been rejected");
            } catch (e: any) {
                expect(e.toString()).to.include("NoFeesToClaim");
            }
        });

        it("rejects a claim from a non-authority caller", async () => {
            const stranger = Keypair.generate();
            const sig = await setup.connection.requestAirdrop(stranger.publicKey, LAMPORTS_PER_SOL);
            await setup.connection.confirmTransaction(sig);
            try {
                await setup.program.methods
                    .claimFees()
                    .accounts({ pool: setup.pool, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("non-authority claim_fees should have been rejected");
            } catch (e: any) {
                expect(e.toString()).to.include("Unauthorized");
            }
        });
    });
});

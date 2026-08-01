import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import { setupTest, TestSetup } from "./utils";

// The fee rate is fixed at 0 — there is no setter — so nothing ever accrues.
// That the accrual path honours the 0 rate is proven with clock control in the
// Rust LiteSVM suite (`programs/calma/tests/test_fee.rs`) and the math unit
// tests. Here we cover the on-chain plumbing through the real Anchor stack: the
// rate a created pool exposes, and the authority-only `claim_fees` guard paths.
describe("protocol fee", () => {
    describe("fee rate", () => {
        let setup: TestSetup;
        before(async () => {
            setup = await setupTest();
        });

        it("is 0 on a created pool and has no setter", async () => {
            const pool = await setup.program.account.pool.fetch(setup.pool);
            expect(pool.market.fee.toString()).to.equal("0");
            expect(setup.program.methods).to.not.have.property("setFee");
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

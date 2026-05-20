import * as anchor from "@anchor-lang/core";
import { PublicKey } from "@solana/web3.js";
import { getAccount } from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, participateInPool, TestSetup } from "./utils";

describe("lp tokens", () => {
    // ── 1. participate mints LP tokens proportionally ─────────────────────────
    describe("participate mints LP tokens to the user", () => {
        const PARTICIPATE_AMOUNT = 100_000_000;

        let setup: TestSetup;
        let userLpTokenAccount: PublicKey;

        before(async () => {
            setup = await setupTest();
            userLpTokenAccount = anchor.utils.token.associatedAddress({
                mint: setup.lpMintPda,
                owner: setup.authority.publicKey,
            });
            await participateInPool(setup, PARTICIPATE_AMOUNT);
        });

        it("mints LP tokens to user wallet after participate", async () => {
            const { connection } = setup;
            const lpAccount = await getAccount(connection, userLpTokenAccount);
            expect(lpAccount.amount.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
        });

        it("pool records total_lp_issued and total_lend_deposited", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalLpIssued.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
            expect(poolAccount.totalLendDeposited.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
        });
    });
});

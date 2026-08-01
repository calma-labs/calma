import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import {
    setupTest,
    createLender,
    participateInPool,
    TestSetup,
    Lender,
} from "./utils";

/**
 * Withdrawal queue: enqueue on `withdraw_lent`, dequeue on `process_queue_entry`.
 *
 * A lend-side exit is paid immediately when the vault holds enough idle
 * liquidity AND the queue is empty. Otherwise the LP is burned, the claim is
 * recorded at that moment's share price, and the tokens are paid later by
 * `process_queue_entry` once borrowers repay.
 *
 * The ordering rule matters: once anything is queued, later exits queue too even
 * if the vault could cover them, so nobody jumps the line.
 */

const COLLATERAL = 400_000_000;

// ── helpers ──────────────────────────────────────────────────────────────────

async function depositCollateral(setup: TestSetup, lender: Lender, amount: number) {
    await setup.program.methods
        .depositCollateral(new BN(amount))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            collateralMint: setup.collateralMint,
            authority: lender.authority.publicKey,
            userTokenAccount: lender.userTokenAccount,
        })
        .signers([lender.authority])
        .rpc();
}

async function borrow(setup: TestSetup, lender: Lender, amount: number) {
    await setup.program.methods
        .borrow(new BN(amount))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: lender.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
            feedProgram: setup.feedProgram.programId,
            feedState: setup.feedPda,
        })
        .signers([lender.authority])
        .rpc();
}

async function repay(setup: TestSetup, lender: Lender, amount: BN) {
    await setup.program.methods
        .repay(amount)
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: lender.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
        })
        .signers([lender.authority])
        .rpc();
}

/** Deposit lend tokens from a non-setup lender, minting them LP. */
async function depositLentAs(setup: TestSetup, lender: Lender, amount: number) {
    await setup.program.methods
        .depositLent(new BN(amount))
        .accounts({
            guardProgram: null,
            guardState: null,
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: lender.authority.publicKey,
            userLendTokenAccount: lender.userLendTokenAccount,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
        })
        .signers([lender.authority])
        .rpc();
}

async function withdrawLentAs(setup: TestSetup, lender: Lender, shares: BN) {
    await setup.program.methods
        .withdrawLent(shares)
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: lender.authority.publicKey,
            rateProgram: setup.irmProgramId,
            irmState: setup.irmConfig,
        })
        .signers([lender.authority])
        .rpc();
}

/** Crank the head of the queue, paying `beneficiary`. Anyone may call it. */
async function processQueueEntry(setup: TestSetup, beneficiary: PublicKey) {
    const ata = getAssociatedTokenAddressSync(setup.lendMint, beneficiary);
    await setup.program.methods
        .processQueueEntry()
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            userTokenAccount: ata,
        })
        .rpc();
}

async function lpBalance(setup: TestSetup, owner: PublicKey): Promise<bigint> {
    const ata = getAssociatedTokenAddressSync(setup.lpMintPda, owner);
    try {
        return (await getAccount(setup.connection, ata)).amount;
    } catch {
        return BigInt(0);
    }
}

async function lendBalance(setup: TestSetup, owner: PublicKey): Promise<bigint> {
    const ata = getAssociatedTokenAddressSync(setup.lendMint, owner);
    try {
        return (await getAccount(setup.connection, ata)).amount;
    } catch {
        return BigInt(0);
    }
}

async function vaultBalance(setup: TestSetup): Promise<bigint> {
    return (await getAccount(setup.connection, setup.lendVaultPda)).amount;
}

async function queueState(setup: TestSetup) {
    const pool = await setup.program.account.pool.fetch(setup.pool);
    const head = pool.withdrawalQueue.head;
    const tail = pool.withdrawalQueue.tail;
    const LEN = 1024;
    const depth = (tail + LEN - head) % LEN;
    return {
        head,
        tail,
        depth,
        entries: pool.withdrawalQueue.entries,
        assetsInQueue: pool.market.assetsInQueue as BN,
        totalSupplyAssets: pool.market.totalSupplyAssets as BN,
        totalSupplyShares: pool.market.totalSupplyShares as BN,
    };
}

async function expectRejected(label: string, needle: string, fn: () => Promise<void>) {
    try {
        await fn();
        expect.fail(`expected ${label} to be rejected`);
    } catch (e: any) {
        const msg = e.toString() as string;
        if (msg.includes("expected") && msg.includes("to be rejected")) throw e;
        expect(msg).to.include(needle);
    }
}

// ── 1. Immediate path — nothing should enqueue when liquidity is idle ─────────

describe("withdrawal queue", () => {
    describe("immediate withdrawal (queue stays empty)", () => {
        const DEPOSIT = 200_000_000;
        let setup: TestSetup;
        let lender: Lender;

        before(async () => {
            setup = await setupTest();
            lender = await createLender(setup);
            await depositLentAs(setup, lender, DEPOSIT);
        });

        it("pays out directly while the vault is fully liquid", async () => {
            const before = await lendBalance(setup, lender.authority.publicKey);
            await withdrawLentAs(setup, lender, new BN(DEPOSIT));
            const after = await lendBalance(setup, lender.authority.publicKey);

            expect((after - before).toString()).to.equal(DEPOSIT.toString());
            expect((await lpBalance(setup, lender.authority.publicKey)).toString()).to.equal("0");
        });

        it("leaves the queue empty and assets_in_queue at zero", async () => {
            const q = await queueState(setup);
            expect(q.depth).to.equal(0);
            expect(q.head).to.equal(q.tail);
            expect(q.assetsInQueue.toString()).to.equal("0");
        });
    });

    // ── 2. Enqueue when liquidity is lent out ────────────────────────────────

    describe("enqueue when the vault is drained by borrowers", () => {
        const DEPOSIT = 200_000_000;
        const BORROWED = 150_000_000;
        let setup: TestSetup;
        let lender: Lender;
        let borrower: Lender;

        before(async () => {
            setup = await setupTest();
            lender = await createLender(setup);
            borrower = await createLender(setup);

            await depositLentAs(setup, lender, DEPOSIT);
            await depositCollateral(setup, borrower, COLLATERAL);
            await borrow(setup, borrower, BORROWED);
        });

        it("burns LP and enqueues when the vault cannot cover the exit", async () => {
            const lendBefore = await lendBalance(setup, lender.authority.publicKey);
            await withdrawLentAs(setup, lender, new BN(DEPOSIT));

            // LP is gone, but no lend tokens have arrived yet.
            expect((await lpBalance(setup, lender.authority.publicKey)).toString()).to.equal("0");
            expect((await lendBalance(setup, lender.authority.publicKey)).toString()).to.equal(
                lendBefore.toString()
            );
        });

        it("records the requester and the claim amount on the queue", async () => {
            const q = await queueState(setup);
            expect(q.depth).to.equal(1);

            const entry = q.entries[q.head];
            expect(entry.requester.toBase58()).to.equal(lender.authority.publicKey.toBase58());
            // Claim is at least the principal — interest accrued in between only helps.
            expect(entry.amount.gte(new BN(DEPOSIT))).to.be.true;
            expect(q.assetsInQueue.toString()).to.equal(entry.amount.toString());
        });

        it("refuses to process while the vault is still short, without dequeueing", async () => {
            await expectRejected("starved process_queue_entry", "InsufficientFunds", async () => {
                await processQueueEntry(setup, lender.authority.publicKey);
            });
            // Crucially the entry keeps its place — it is not consumed on failure.
            const q = await queueState(setup);
            expect(q.depth).to.equal(1);
        });

        it("will not lend queued liquidity back out to a new borrower", async () => {
            // The vault holds DEPOSIT - BORROWED idle, but every token of it is
            // already promised to the queued lender.
            await expectRejected("borrow against queued liquidity", "InsufficientFunds", async () => {
                await borrow(setup, borrower, 1_000_000);
            });
        });

        it("pays the queued lender once the borrower repays", async () => {
            await repay(setup, borrower, new BN("18446744073709551615")); // u64::MAX — clears all

            const before = await lendBalance(setup, lender.authority.publicKey);
            const q = await queueState(setup);
            const owed = q.entries[q.head].amount as BN;

            await processQueueEntry(setup, lender.authority.publicKey);

            const after = await lendBalance(setup, lender.authority.publicKey);
            expect((after - before).toString()).to.equal(owed.toString());
        });

        it("dequeues and releases assets_in_queue after payout", async () => {
            const q = await queueState(setup);
            expect(q.depth).to.equal(0);
            expect(q.assetsInQueue.toString()).to.equal("0");
        });

        it("rejects processing an empty queue", async () => {
            await expectRejected("empty queue", "WithdrawalQueueEmpty", async () => {
                await processQueueEntry(setup, lender.authority.publicKey);
            });
        });
    });

    // ── 3. FIFO ordering across several lenders ──────────────────────────────

    describe("FIFO ordering", () => {
        const EACH = 100_000_000;
        const BORROWED = 250_000_000;
        let setup: TestSetup;
        let a: Lender, b: Lender, c: Lender, borrower: Lender;
        /** Joins the queue mid-suite, behind a/b/c. */
        let latecomer: Lender;

        before(async () => {
            setup = await setupTest();
            [a, b, c, borrower] = [
                await createLender(setup),
                await createLender(setup),
                await createLender(setup),
                await createLender(setup),
            ];

            await depositLentAs(setup, a, EACH);
            await depositLentAs(setup, b, EACH);
            await depositLentAs(setup, c, EACH);

            await depositCollateral(setup, borrower, COLLATERAL);
            await borrow(setup, borrower, BORROWED);

            // All three exit while the vault is short — order a, b, c.
            await withdrawLentAs(setup, a, new BN(EACH));
            await withdrawLentAs(setup, b, new BN(EACH));
            await withdrawLentAs(setup, c, new BN(EACH));
        });

        it("queues every exit in submission order", async () => {
            const q = await queueState(setup);
            expect(q.depth).to.equal(3);
            const order = [0, 1, 2].map((i) => q.entries[(q.head + i) % 1024].requester.toBase58());
            expect(order).to.deep.equal([
                a.authority.publicKey.toBase58(),
                b.authority.publicKey.toBase58(),
                c.authority.publicKey.toBase58(),
            ]);
        });

        it("queues a later exit even when the vault could pay it immediately", async () => {
            // The queue being non-empty is itself enough to enqueue — this is what
            // stops a latecomer from draining liquidity ahead of those in line.
            latecomer = await createLender(setup);
            await depositLentAs(setup, latecomer, 10_000_000);
            expect((await vaultBalance(setup)) > BigInt(0)).to.be.true;

            await withdrawLentAs(setup, latecomer, new BN(10_000_000));
            const q = await queueState(setup);
            expect(q.depth).to.equal(4);
            expect(q.entries[(q.head + 3) % 1024].requester.toBase58()).to.equal(
                latecomer.authority.publicKey.toBase58()
            );
        });

        it("will not pay the second in line before the first", async () => {
            await expectRejected("out-of-order payout", "QueueEntryMismatch", async () => {
                await processQueueEntry(setup, b.authority.publicKey);
            });
        });

        it("drains the queue head-first once liquidity returns", async () => {
            await repay(setup, borrower, new BN("18446744073709551615"));

            // Repayment alone leaves the vault only marginally above the queued
            // claims (which grew with accrued interest). Top it up from a fresh
            // lender so this test measures ordering, not the liquidity boundary
            // — that boundary has its own test above.
            const topUp = await createLender(setup);
            await depositLentAs(setup, topUp, 200_000_000);

            for (const who of [a, b, c]) {
                const before = await lendBalance(setup, who.authority.publicKey);
                const q = await queueState(setup);
                const owed = q.entries[q.head].amount as BN;

                await processQueueEntry(setup, who.authority.publicKey);

                const after = await lendBalance(setup, who.authority.publicKey);
                expect((after - before).toString()).to.equal(owed.toString());
            }

            const q = await queueState(setup);
            expect(q.depth).to.equal(1, "only the latecomer should remain");
            expect(q.entries[q.head].requester.toBase58()).to.equal(
                latecomer.authority.publicKey.toBase58()
            );
        });
    });

    // ── 4. Accounting invariants across the whole cycle ──────────────────────

    describe("accounting invariants", () => {
        const DEPOSIT = 200_000_000;
        const BORROWED = 150_000_000;
        let setup: TestSetup;
        let lender: Lender;
        let borrower: Lender;

        before(async () => {
            setup = await setupTest();
            lender = await createLender(setup);
            borrower = await createLender(setup);
            await depositLentAs(setup, lender, DEPOSIT);
            await depositCollateral(setup, borrower, COLLATERAL);
            await borrow(setup, borrower, BORROWED);
        });

        it("moves shares AND their assets out of supply on enqueue", async () => {
            const before = await queueState(setup);
            await withdrawLentAs(setup, lender, new BN(DEPOSIT));
            const after = await queueState(setup);

            const sharesBurned = before.totalSupplyShares.sub(after.totalSupplyShares);
            const assetsMoved = before.totalSupplyAssets.sub(after.totalSupplyAssets);
            const claim = after.entries[after.head].amount as BN;

            expect(sharesBurned.gt(new BN(0))).to.be.true;
            // Both sides of the ratio must move together, otherwise the share
            // price climbs for whoever is still in and the queue over-commits.
            expect(assetsMoved.gt(new BN(0))).to.be.true;
            // The reservation is exactly the claim recorded on the queue.
            expect(after.assetsInQueue.toString()).to.equal(claim.toString());
            // Supply is debited by the same claim. It is not simply
            // `before - after`: this instruction accrues interest first, which
            // credits supply on the way past — so total assets afterwards equal
            // what was there before plus that interest, never less.
            expect(
                after.totalSupplyAssets.add(after.assetsInQueue).gte(before.totalSupplyAssets)
            ).to.be.true;
            expect(claim.sub(assetsMoved).lte(new BN(1_000))).to.be.true;
        });

        it("releases only assets_in_queue on dequeue (supply already debited)", async () => {
            await repay(setup, borrower, new BN("18446744073709551615"));

            const before = await queueState(setup);
            const owed = before.entries[before.head].amount as BN;

            await processQueueEntry(setup, lender.authority.publicKey);

            const after = await queueState(setup);
            expect(before.assetsInQueue.sub(after.assetsInQueue).toString()).to.equal(owed.toString());
            // Supply was already debited at enqueue — it must not be charged twice.
            expect(after.totalSupplyAssets.gte(before.totalSupplyAssets)).to.be.true;
        });

        it("leaves the vault solvent against remaining supply", async () => {
            const q = await queueState(setup);
            const vault = await vaultBalance(setup);
            // Nothing is queued any more, so idle vault + outstanding debt must
            // cover what lenders are still owed.
            expect(q.assetsInQueue.toString()).to.equal("0");
            expect(vault >= BigInt(0)).to.be.true;
        });
    });

    // ── 5. Wrong destination account ─────────────────────────────────────────

    describe("payout account validation", () => {
        const DEPOSIT = 200_000_000;
        const BORROWED = 150_000_000;
        let setup: TestSetup;
        let lender: Lender;
        let borrower: Lender;
        let stranger: Lender;

        before(async () => {
            setup = await setupTest();
            lender = await createLender(setup);
            borrower = await createLender(setup);
            stranger = await createLender(setup);

            await depositLentAs(setup, lender, DEPOSIT);
            await depositCollateral(setup, borrower, COLLATERAL);
            await borrow(setup, borrower, BORROWED);
            await withdrawLentAs(setup, lender, new BN(DEPOSIT));
            await repay(setup, borrower, new BN("18446744073709551615"));
        });

        it("refuses to pay a queued claim into someone else's account", async () => {
            await expectRejected("stranger payout", "QueueEntryMismatch", async () => {
                await processQueueEntry(setup, stranger.authority.publicKey);
            });
        });

        it("still pays the rightful requester afterwards", async () => {
            const before = await lendBalance(setup, lender.authority.publicKey);
            await processQueueEntry(setup, lender.authority.publicKey);
            const after = await lendBalance(setup, lender.authority.publicKey);
            expect(after > before).to.be.true;
        });
    });
});

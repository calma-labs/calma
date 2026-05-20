import * as anchor from "@anchor-lang/core";
import { getAccount } from "@solana/spl-token";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import { setupTest, createLender, participateInPool, TestSetup } from "./utils";

/** Deposited by setup.authority into the lend vault so borrowers have something to borrow. */
const LEND_LIQUIDITY = 500_000_000; // 500 lend tokens

async function depositCollateral(setup: TestSetup, authority: anchor.web3.Keypair, userTokenAccount: anchor.web3.PublicKey, amount: number) {
    await setup.program.methods
        .depositCollateral(new anchor.BN(amount))
        .accounts({
            pool: setup.pool,
            collateralMint: setup.collateralMint,
            authority: authority.publicKey,
            userTokenAccount,
        })
        .signers([authority])
        .rpc();
}

async function borrow(setup: TestSetup, authority: anchor.web3.Keypair, amount: number) {
    await setup.program.methods
        .borrow(new anchor.BN(amount))
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: authority.publicKey,
        })
        .signers([authority])
        .rpc();
}

async function repay(setup: TestSetup, authority: anchor.web3.Keypair, amount: number) {
    await setup.program.methods
        .repay(new anchor.BN(amount))
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            authority: authority.publicKey,
        })
        .signers([authority])
        .rpc();
}

describe("borrow", () => {
    // ── 1. Happy path ─────────────────────────────────────────────────────────────
    describe("borrow within LTV", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const BORROW_AMOUNT = 50_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
        });

        it("transfers lend tokens to borrower and updates pool and position state", async () => {
            const { program, pool, lendVaultPda, userPositionPda, authority, connection } = setup;

            await borrow(setup, authority, BORROW_AMOUNT);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toString()).to.equal(BORROW_AMOUNT.toString());
            expect(poolAccount.totalDebtShares.toNumber()).to.be.greaterThan(0);

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.be.greaterThan(0);

            const lendVault = await getAccount(connection, lendVaultPda);
            expect(Number(lendVault.amount)).to.equal(LEND_LIQUIDITY - BORROW_AMOUNT);

            // Borrower's lend ATA receives the borrowed tokens.
            const borrowerLendAta = anchor.utils.token.associatedAddress({
                mint: setup.lendMint,
                owner: authority.publicKey,
            });
            const userLendToken = await getAccount(connection, borrowerLendAta);
            // Authority started with 1_000_000_000 lend tokens, participated with LEND_LIQUIDITY,
            // then borrowed BORROW_AMOUNT back.
            expect(Number(userLendToken.amount)).to.equal(1_000_000_000 - LEND_LIQUIDITY + BORROW_AMOUNT);
        });
    });

    // ── 2. Exact LTV ceiling ──────────────────────────────────────────────────────
    describe("borrow at exact 75% LTV ceiling", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const MAX_BORROW = 75_000_000; // exactly 75%

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
        });

        it("succeeds borrowing exactly 75% of deposited collateral", async () => {
            const { program, pool, userPositionPda, authority } = setup;

            await borrow(setup, authority, MAX_BORROW);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toString()).to.equal(MAX_BORROW.toString());

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.be.greaterThan(0);
        });
    });

    // ── 3. Over LTV by 1 token ────────────────────────────────────────────────────
    describe("borrow exceeding LTV by 1 token", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const OVER_LTV = 75_000_001;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
        });

        it("rejects a borrow 1 token over the 75% LTV limit", async () => {
            try {
                await borrow(setup, setup.authority, OVER_LTV);
                expect.fail("expected borrow to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InsufficientFunds");
            }
        });
    });

    // ── 4. Zero amount ────────────────────────────────────────────────────────────
    describe("borrow of zero tokens", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
        });

        it("rejects borrow(0) with InvalidAmount", async () => {
            try {
                await borrow(setup, setup.authority, 0);
                expect.fail("expected borrow to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InvalidAmount");
            }
        });
    });

    // ── 5. No prior collateral deposit ───────────────────────────────────────────
    describe("borrow without a prior collateral deposit", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
        });

        it("rejects a borrow when the user has no position account", async () => {
            const { connection } = setup;

            const stranger = Keypair.generate();
            const airdrop = await connection.requestAirdrop(stranger.publicKey, 2 * LAMPORTS_PER_SOL);
            await connection.confirmTransaction(airdrop);

            try {
                await setup.program.methods
                    .borrow(new anchor.BN(50_000_000))
                    .accounts({ pool: setup.pool, lendMint: setup.lendMint, authority: stranger.publicKey })
                    .signers([stranger])
                    .rpc();
                expect.fail("expected borrow to be rejected");
            } catch (e: any) {
                expect(e).to.exist;
            }
        });
    });

    // ── 6. Multiple sequential borrows ───────────────────────────────────────────
    describe("multiple sequential borrows on the same position", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const FIRST_BORROW = 30_000_000;
        const SECOND_BORROW = 20_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
        });

        it("first borrow succeeds", async () => {
            const { program, pool, userPositionPda, authority } = setup;

            await borrow(setup, authority, FIRST_BORROW);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toString()).to.equal(FIRST_BORROW.toString());

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.be.greaterThan(0);
        });

        it("second borrow on the same position accumulates correctly", async () => {
            const { program, pool, lendVaultPda, userPositionPda, authority, connection } = setup;

            const positionBefore = await program.account.userPosition.fetch(userPositionPda);
            const sharesBefore = positionBefore.debtShares.toNumber();

            await borrow(setup, authority, SECOND_BORROW);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toNumber()).to.be.greaterThanOrEqual(FIRST_BORROW + SECOND_BORROW);

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.be.greaterThan(sharesBefore);

            const lendVault = await getAccount(connection, lendVaultPda);
            expect(Number(lendVault.amount)).to.equal(LEND_LIQUIDITY - FIRST_BORROW - SECOND_BORROW);
        });
    });

    // ── 7. Second borrow pushes over LTV ─────────────────────────────────────────
    describe("second borrow would push total debt over LTV", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const FIRST_BORROW = 60_000_000;
        const SECOND_BORROW = 16_000_000; // 60 + 16 = 76 > 75 (LTV max)

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
            await borrow(setup, setup.authority, FIRST_BORROW);
        });

        it("rejects a second borrow that would push total debt over 75% LTV", async () => {
            try {
                await borrow(setup, setup.authority, SECOND_BORROW);
                expect.fail("expected second borrow to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InsufficientFunds");
            }
        });
    });

    // ── 8. Multiple concurrent borrowers ─────────────────────────────────────────
    describe("multiple concurrent borrowers from the same pool", () => {
        const COLLATERAL_A = 100_000_000;
        const COLLATERAL_B = 200_000_000;
        const BORROW_A = 50_000_000;
        const BORROW_B = 100_000_000;

        let setup: TestSetup;
        let borrowerA: Awaited<ReturnType<typeof createLender>>;
        let borrowerB: Awaited<ReturnType<typeof createLender>>;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            [borrowerA, borrowerB] = await Promise.all([createLender(setup), createLender(setup)]);

            await depositCollateral(setup, borrowerA.authority, borrowerA.userTokenAccount, COLLATERAL_A);
            await depositCollateral(setup, borrowerB.authority, borrowerB.userTokenAccount, COLLATERAL_B);
        });

        it("borrower A borrows within their LTV", async () => {
            const { program, pool, lendVaultPda, connection } = setup;

            await borrow(setup, borrowerA.authority, BORROW_A);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toNumber()).to.be.greaterThanOrEqual(BORROW_A);

            const lendVault = await getAccount(connection, lendVaultPda);
            expect(Number(lendVault.amount)).to.equal(LEND_LIQUIDITY - BORROW_A);
        });

        it("borrower B borrows within their LTV — pool totals accumulate", async () => {
            const { program, pool, lendVaultPda, connection } = setup;

            await borrow(setup, borrowerB.authority, BORROW_B);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toNumber()).to.be.greaterThanOrEqual(BORROW_A + BORROW_B);
            expect(poolAccount.totalDebtShares.toNumber()).to.be.greaterThan(0);

            const positionA = await program.account.userPosition.fetch(borrowerA.userPositionPda);
            expect(positionA.debtShares.toNumber()).to.be.greaterThan(0);

            const positionB = await program.account.userPosition.fetch(borrowerB.userPositionPda);
            expect(positionB.debtShares.toNumber()).to.be.greaterThan(positionA.debtShares.toNumber());

            const lendVault = await getAccount(connection, lendVaultPda);
            expect(Number(lendVault.amount)).to.equal(LEND_LIQUIDITY - BORROW_A - BORROW_B);
        });
    });

    // ── 9. Repay full debt ────────────────────────────────────────────────────────
    describe("repay full outstanding debt", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const BORROW_AMOUNT = 50_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
            await borrow(setup, setup.authority, BORROW_AMOUNT);
        });

        it("clears debt shares and returns lend tokens to vault", async () => {
            const { program, pool, lendVaultPda, userPositionPda, authority, connection } = setup;

            // Repay with a large amount; handler caps to actual debt owed.
            await repay(setup, authority, BORROW_AMOUNT * 2);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toNumber()).to.equal(0);
            expect(poolAccount.totalDebtShares.toNumber()).to.equal(0);

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.equal(0);

            const lendVault = await getAccount(connection, lendVaultPda);
            // Vault recovers at least LEND_LIQUIDITY; repay can include accrued interest.
            expect(Number(lendVault.amount)).to.be.greaterThanOrEqual(LEND_LIQUIDITY);
        });
    });

    // ── 10. Partial repay ────────────────────────────────────────────────────────
    describe("partial repay leaves remaining debt", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const BORROW_AMOUNT = 60_000_000;
        const PARTIAL_REPAY = 30_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
            await borrow(setup, setup.authority, BORROW_AMOUNT);
        });

        it("reduces debt shares and lend vault balance after partial repay", async () => {
            const { program, pool, lendVaultPda, userPositionPda, authority, connection } = setup;

            const positionBefore = await program.account.userPosition.fetch(userPositionPda);

            await repay(setup, authority, PARTIAL_REPAY);

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.debtShares.toNumber()).to.be.greaterThan(0);
            expect(position.debtShares.toNumber()).to.be.lessThan(positionBefore.debtShares.toNumber());

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalBorrowed.toNumber()).to.be.greaterThan(0);
            expect(poolAccount.totalBorrowed.toNumber()).to.be.lessThan(BORROW_AMOUNT);
        });
    });

    // ── 11. Repay exact full debt clears all shares (rounding test) ───────────────
    describe("repay exact calculated debt amount clears all shares", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const BORROW_AMOUNT = 50_000_000;

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest({
                m1: new anchor.BN(0),
                c1: new anchor.BN(0),
                m2: new anchor.BN(0),
                c2: new anchor.BN(0),
            });
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
            await borrow(setup, setup.authority, BORROW_AMOUNT);
        });

        it("fully repays debt and clears all debt shares", async () => {
            const { program, pool, userPositionPda, authority } = setup;

            // Get position before repay
            const positionBefore = await program.account.userPosition.fetch(userPositionPda);
            const poolBefore = await program.account.pool.fetch(pool);

            // Calculate exact debt amount using BigInt for precision (matches program's ceiling division)
            const debtShares = BigInt(positionBefore.debtShares.toString());
            const totalBorrowed = BigInt(poolBefore.totalBorrowed.toString());
            const totalDebtShares = BigInt(poolBefore.totalDebtShares.toString());
            // shares_to_amount uses ceiling division: (shares * total_borrowed + total_debt_shares - 1) / total_debt_shares
            const exactDebt = Number((debtShares * totalBorrowed + totalDebtShares - BigInt(1)) / totalDebtShares);

            // Repay the exact calculated debt
            await repay(setup, authority, exactDebt);

            // Verify all debt is cleared
            const positionAfter = await program.account.userPosition.fetch(userPositionPda);
            expect(positionAfter.debtShares.toNumber()).to.equal(0);

            const poolAfter = await program.account.pool.fetch(pool);
            expect(poolAfter.totalDebtShares.toNumber()).to.equal(0);
            expect(poolAfter.totalBorrowed.toNumber()).to.equal(0);
        });
    });

    // ── 12. Withdraw collateral blocked by open borrow ────────────────────────────
    describe("withdraw collateral with open borrow", () => {
        const COLLATERAL_DEPOSIT = 100_000_000;
        const BORROW_AMOUNT = 70_000_000; // 70% LTV — withdraw would breach

        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, COLLATERAL_DEPOSIT);
            await borrow(setup, setup.authority, BORROW_AMOUNT);
        });

        it("rejects full collateral withdraw when borrow is open (LTV breach)", async () => {
            try {
                await setup.program.methods
                    .withdrawCollateral(new anchor.BN(COLLATERAL_DEPOSIT))
                    .accounts({
                        pool: setup.pool,
                        collateralMint: setup.collateralMint,
                        authority: setup.authority.publicKey,
                        userTokenAccount: setup.userCollateralTokenAccount,
                    })
                    .signers([setup.authority])
                    .rpc();
                expect.fail("expected withdraw to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InsufficientFunds");
            }
        });
    });
});


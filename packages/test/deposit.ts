import * as anchor from "@anchor-lang/core";
import { getAccount } from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, createLender, TestSetup } from "./utils";

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

async function withdrawCollateral(setup: TestSetup, authority: anchor.web3.Keypair, userTokenAccount: anchor.web3.PublicKey, amount: number) {
    await setup.program.methods
        .withdrawCollateral(new anchor.BN(amount))
        .accounts({
            pool: setup.pool,
            collateralMint: setup.collateralMint,
            authority: authority.publicKey,
            userTokenAccount,
        })
        .signers([authority])
        .rpc();
}

describe("deposit and withdraw", () => {
    // ── 1. Simple collateral deposit then full withdraw ───────────────────────────
    describe("simple deposit and full withdraw", () => {
        const DEPOSIT_AMOUNT = 100_000_000; // 100 tokens (6 decimals)
        const INITIAL_BALANCE = 1_000_000_000; // 1000 tokens minted in setup

        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
        });

        it("asserts pool and position state after collateral deposit", async () => {
            const { program, collateralMint, lendMint, pool, collateralVaultPda, lpMintPda, userPositionPda, userCollateralTokenAccount, authority, connection } = setup;

            await depositCollateral(setup, authority, userCollateralTokenAccount, DEPOSIT_AMOUNT);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.authority.toString()).to.equal(authority.publicKey.toString());
            expect(poolAccount.collateralMint.toString()).to.equal(collateralMint.toString());
            expect(poolAccount.lendMint.toString()).to.equal(lendMint.toString());
            expect(poolAccount.lpMint.toString()).to.equal(lpMintPda.toString());
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal(DEPOSIT_AMOUNT.toString());
            expect(poolAccount.totalLendDeposited.toString()).to.equal("0");
            expect(poolAccount.totalLpIssued.toString()).to.equal("0"); // LP only issued via participate
            expect(poolAccount.totalBorrowed.toString()).to.equal("0");
            expect(poolAccount.totalDebtShares.toString()).to.equal("0");
            expect(poolAccount.ltvPercent).to.equal(75);

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.authority.toString()).to.equal(authority.publicKey.toString());
            expect(position.pool.toString()).to.equal(pool.toString());
            expect(position.collateralDeposited.toString()).to.equal(DEPOSIT_AMOUNT.toString());
            expect(position.debtShares.toString()).to.equal("0");

            const vault = await getAccount(connection, collateralVaultPda);
            expect(vault.amount.toString()).to.equal(DEPOSIT_AMOUNT.toString());

            const userToken = await getAccount(connection, userCollateralTokenAccount);
            expect(userToken.amount.toString()).to.equal((INITIAL_BALANCE - DEPOSIT_AMOUNT).toString());
        });

        it("asserts pool and position state after full collateral withdraw", async () => {
            const { program, pool, collateralVaultPda, userPositionPda, userCollateralTokenAccount, authority, connection } = setup;

            await withdrawCollateral(setup, authority, userCollateralTokenAccount, DEPOSIT_AMOUNT);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal("0");
            expect(poolAccount.totalBorrowed.toString()).to.equal("0");

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.collateralDeposited.toString()).to.equal("0");
            expect(position.debtShares.toString()).to.equal("0");

            const vault = await getAccount(connection, collateralVaultPda);
            expect(vault.amount.toString()).to.equal("0");

            const userToken = await getAccount(connection, userCollateralTokenAccount);
            expect(userToken.amount.toString()).to.equal(INITIAL_BALANCE.toString());
        });
    });

    // ── 2. Partial withdraw ───────────────────────────────────────────────────────
    describe("partial withdraw", () => {
        const DEPOSIT_AMOUNT = 100_000_000;
        const FIRST_WITHDRAW = 40_000_000;
        const SECOND_WITHDRAW = 60_000_000;
        const INITIAL_BALANCE = 1_000_000_000;

        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, DEPOSIT_AMOUNT);
        });

        it("asserts correct state after partial withdraw (40 of 100 tokens)", async () => {
            const { program, pool, collateralVaultPda, userPositionPda, userCollateralTokenAccount, authority, connection } = setup;

            await withdrawCollateral(setup, authority, userCollateralTokenAccount, FIRST_WITHDRAW);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal((DEPOSIT_AMOUNT - FIRST_WITHDRAW).toString());
            expect(poolAccount.totalBorrowed.toString()).to.equal("0");

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.collateralDeposited.toString()).to.equal((DEPOSIT_AMOUNT - FIRST_WITHDRAW).toString());
            expect(position.debtShares.toString()).to.equal("0");

            const vault = await getAccount(connection, collateralVaultPda);
            expect(vault.amount.toString()).to.equal((DEPOSIT_AMOUNT - FIRST_WITHDRAW).toString());

            const userToken = await getAccount(connection, userCollateralTokenAccount);
            expect(userToken.amount.toString()).to.equal((INITIAL_BALANCE - DEPOSIT_AMOUNT + FIRST_WITHDRAW).toString());
        });

        it("asserts correct state after withdrawing the remainder (60 tokens)", async () => {
            const { program, pool, collateralVaultPda, userPositionPda, userCollateralTokenAccount, authority, connection } = setup;

            await withdrawCollateral(setup, authority, userCollateralTokenAccount, SECOND_WITHDRAW);

            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal("0");

            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.collateralDeposited.toString()).to.equal("0");

            const vault = await getAccount(connection, collateralVaultPda);
            expect(vault.amount.toString()).to.equal("0");

            const userToken = await getAccount(connection, userCollateralTokenAccount);
            expect(userToken.amount.toString()).to.equal(INITIAL_BALANCE.toString());
        });
    });

    // ── 3. Multiple depositors in the same pool ───────────────────────────────────
    describe("multiple depositors in the same pool", () => {
        const DEPOSIT_A = 100_000_000;
        const DEPOSIT_B = 200_000_000;
        const WITHDRAW_A = 100_000_000;
        const WITHDRAW_B_PARTIAL = 80_000_000;
        const INITIAL_BALANCE = 1_000_000_000;

        let setup: Awaited<ReturnType<typeof setupTest>>;
        let depositorA: Awaited<ReturnType<typeof createLender>>;
        let depositorB: Awaited<ReturnType<typeof createLender>>;

        before(async () => {
            setup = await setupTest();
            [depositorA, depositorB] = await Promise.all([createLender(setup), createLender(setup)]);

            await depositCollateral(setup, depositorA.authority, depositorA.userTokenAccount, DEPOSIT_A);
            await depositCollateral(setup, depositorB.authority, depositorB.userTokenAccount, DEPOSIT_B);
        });

        it("asserts pool totals reflect both deposits", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal((DEPOSIT_A + DEPOSIT_B).toString());
            expect(poolAccount.totalBorrowed.toString()).to.equal("0");

            const positionA = await setup.program.account.userPosition.fetch(depositorA.userPositionPda);
            expect(positionA.collateralDeposited.toString()).to.equal(DEPOSIT_A.toString());

            const positionB = await setup.program.account.userPosition.fetch(depositorB.userPositionPda);
            expect(positionB.collateralDeposited.toString()).to.equal(DEPOSIT_B.toString());

            const vault = await getAccount(setup.connection, setup.collateralVaultPda);
            expect(vault.amount.toString()).to.equal((DEPOSIT_A + DEPOSIT_B).toString());
        });

        it("depositor A does a full withdraw — pool reflects only depositor B remaining", async () => {
            await withdrawCollateral(setup, depositorA.authority, depositorA.userTokenAccount, WITHDRAW_A);

            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal(DEPOSIT_B.toString());

            const positionA = await setup.program.account.userPosition.fetch(depositorA.userPositionPda);
            expect(positionA.collateralDeposited.toString()).to.equal("0");

            const userTokenA = await getAccount(setup.connection, depositorA.userTokenAccount);
            expect(userTokenA.amount.toString()).to.equal(INITIAL_BALANCE.toString());

            const vault = await getAccount(setup.connection, setup.collateralVaultPda);
            expect(vault.amount.toString()).to.equal(DEPOSIT_B.toString());
        });

        it("depositor B does a partial withdraw — correct remaining pool state", async () => {
            await withdrawCollateral(setup, depositorB.authority, depositorB.userTokenAccount, WITHDRAW_B_PARTIAL);

            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal((DEPOSIT_B - WITHDRAW_B_PARTIAL).toString());

            const positionB = await setup.program.account.userPosition.fetch(depositorB.userPositionPda);
            expect(positionB.collateralDeposited.toString()).to.equal((DEPOSIT_B - WITHDRAW_B_PARTIAL).toString());

            const vault = await getAccount(setup.connection, setup.collateralVaultPda);
            expect(vault.amount.toString()).to.equal((DEPOSIT_B - WITHDRAW_B_PARTIAL).toString());

            const userTokenB = await getAccount(setup.connection, depositorB.userTokenAccount);
            expect(userTokenB.amount.toString()).to.equal((INITIAL_BALANCE - DEPOSIT_B + WITHDRAW_B_PARTIAL).toString());
        });
    });

    // ── 4. Withdraw more than deposited ───────────────────────────────────────────
    describe("withdraw more than deposited", () => {
        const DEPOSIT_AMOUNT = 50_000_000;

        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, DEPOSIT_AMOUNT);
        });

        it("rejects withdrawing more than deposited with InsufficientFunds", async () => {
            try {
                await withdrawCollateral(setup, setup.authority, setup.userCollateralTokenAccount, DEPOSIT_AMOUNT + 1);
                expect.fail("expected withdraw to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InsufficientFunds");
            }
        });
    });

    // ── 5. Zero amount deposit is rejected ────────────────────────────────────────
    describe("deposit zero tokens", () => {
        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
        });

        it("rejects deposit(0) with InvalidAmount", async () => {
            try {
                await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, 0);
                expect.fail("expected deposit to be rejected");
            } catch (e: any) {
                expect(e.message).to.include("InvalidAmount");
            }
        });
    });

    // ── 6. Withdraw exact full collateral amount from position ────────────────────
    describe("withdraw exact full collateral amount read from position", () => {
        const DEPOSIT_AMOUNT = 100_000_000;
        const INITIAL_BALANCE = 1_000_000_000;

        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            await depositCollateral(setup, setup.authority, setup.userCollateralTokenAccount, DEPOSIT_AMOUNT);
        });

        it("withdraws the exact collateralDeposited amount from position", async () => {
            const { program, pool, collateralVaultPda, userPositionPda, userCollateralTokenAccount, authority, connection } = setup;

            // Read the actual deposited amount from the position
            const positionBefore = await program.account.userPosition.fetch(userPositionPda);
            const exactCollateralDeposited = positionBefore.collateralDeposited.toNumber();

            // Withdraw the exact amount read from the position
            await withdrawCollateral(setup, authority, userCollateralTokenAccount, exactCollateralDeposited);

            // Verify state after withdraw
            const poolAccount = await program.account.pool.fetch(pool);
            expect(poolAccount.totalCollateralDeposited.toString()).to.equal("0");

            const positionAfter = await program.account.userPosition.fetch(userPositionPda);
            expect(positionAfter.collateralDeposited.toString()).to.equal("0");

            const vault = await getAccount(connection, collateralVaultPda);
            expect(vault.amount.toString()).to.equal("0");

            const userToken = await getAccount(connection, userCollateralTokenAccount);
            expect(userToken.amount.toString()).to.equal(INITIAL_BALANCE.toString());
        });
    });
});

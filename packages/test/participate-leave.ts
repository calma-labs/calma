import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import { PublicKey } from "@solana/web3.js";
import {
    getAccount,
    getMint,
} from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, participateInPool, TestSetup } from "./utils";

const PARTICIPATE_AMOUNT = 1_000_000; // 1 token (6 decimals)

describe("participate and leave", () => {
    // ── 1. Happy path: participate ──────────────────────────────────────────────
    describe("participate with 1_000_000 lend tokens", () => {
        let setup: Awaited<ReturnType<typeof setupTest>>;
        let userLpTokenAccount: PublicKey;

        before(async () => {
            setup = await setupTest();

            userLpTokenAccount = anchor.utils.token.associatedAddress({
                mint: setup.lpMintPda,
                owner: setup.authority.publicKey,
            });
        });

        it("mints LP tokens equal to amount on first deposit", async () => {
            await participateInPool(setup, PARTICIPATE_AMOUNT);

            const lpAccount = await getAccount(setup.connection, userLpTokenAccount);
            expect(lpAccount.amount.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
        });

        it("increases lend_vault token balance by the deposited amount", async () => {
            const vaultToken = await getAccount(setup.connection, setup.lendVaultPda);
            expect(vaultToken.amount.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
        });

        it("increments pool.totalLpIssued by the minted LP amount", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalLpIssued.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
            expect(poolAccount.totalLendDeposited.toString()).to.equal(PARTICIPATE_AMOUNT.toString());
        });

        it("decrements authority lend token account by the deposited amount", async () => {
            const userLend = await getAccount(setup.connection, setup.userLendTokenAccount);
            expect(Number(userLend.amount)).to.equal(1_000_000_000 - PARTICIPATE_AMOUNT);
        });
    });

    // ── 2. Happy path: leave (full redemption, immediate) ──────────────────────
    describe("leave with all shares after participating (immediate path)", () => {
        let setup: Awaited<ReturnType<typeof setupTest>>;
        let userLpTokenAccount: PublicKey;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, PARTICIPATE_AMOUNT);

            userLpTokenAccount = anchor.utils.token.associatedAddress({
                mint: setup.lpMintPda,
                owner: setup.authority.publicKey,
            });
        });

        it("returns all lend tokens to the user and burns LP tokens", async () => {
            const { program, pool, lendMint, authority, connection } = setup;

            const lendBefore = await getAccount(connection, setup.userLendTokenAccount);
            const lendBalanceBefore = Number(lendBefore.amount);

            await program.methods
                .withdrawLent(new BN(PARTICIPATE_AMOUNT))
                .accounts({
                    pool,
                    lendMint,
                    authority: authority.publicKey,
                })
                .signers([authority])
                .rpc();

            const lendAfter = await getAccount(connection, setup.userLendTokenAccount);
            expect(Number(lendAfter.amount) - lendBalanceBefore).to.equal(PARTICIPATE_AMOUNT);

            const lpAccount = await getAccount(connection, userLpTokenAccount);
            expect(lpAccount.amount.toString()).to.equal("0");
        });

        it("empties the lend vault after full redemption", async () => {
            const vaultToken = await getAccount(setup.connection, setup.lendVaultPda);
            expect(vaultToken.amount.toString()).to.equal("0");
        });

        it("decrements pool.totalLpIssued to 0 after full redemption", async () => {
            const poolAccount = await setup.program.account.pool.fetch(setup.pool);
            expect(poolAccount.totalLpIssued.toString()).to.equal("0");
        });
    });

    // ── 3. Second depositor gets proportional LP ──────────────────────────────
    describe("second participate is proportional to existing deposits", () => {
        const FIRST_AMOUNT = 1_000_000;
        const SECOND_AMOUNT = 2_000_000;

        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, FIRST_AMOUNT);
        });

        it("mints 2x LP for 2x deposit on second participate", async () => {
            const { pool, authority } = setup;

            // Authority has already participated FIRST_AMOUNT; now participates again.
            await participateInPool(setup, SECOND_AMOUNT);

            const poolAccount = await setup.program.account.pool.fetch(pool);
            // totalLpIssued = FIRST_AMOUNT + SECOND_AMOUNT (1:1 and 2:1 ratio gives 1M + 2M = 3M)
            expect(poolAccount.totalLpIssued.toString()).to.equal((FIRST_AMOUNT + SECOND_AMOUNT).toString());
            expect(poolAccount.totalLendDeposited.toString()).to.equal((FIRST_AMOUNT + SECOND_AMOUNT).toString());
        });
    });

    // ── 4. Error: leave with 0 shares ───────────────────────────────────────────
    describe("leave with 0 shares", () => {
        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            // Participate so the LP ATA exists on-chain (leave needs it to exist).
            await participateInPool(setup, PARTICIPATE_AMOUNT);
        });

        it("fails with InvalidAmount when shares = 0", async () => {
            const { program, pool, lendMint, authority } = setup;

            try {
                await program.methods
                    .withdrawLent(new BN(0))
                    .accounts({ pool, lendMint, authority: authority.publicKey })
                    .signers([authority])
                    .rpc();
                expect.fail("Expected leave(0) to fail with InvalidAmount");
            } catch (err: unknown) {
                expect(err).to.be.instanceOf(Error);
                expect((err as Error).message).to.include("InvalidAmount");
            }
        });
    });

    // ── 5. Error: leave with more shares than LP balance ────────────────────────
    describe("leave with more shares than LP balance", () => {
        let setup: Awaited<ReturnType<typeof setupTest>>;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, PARTICIPATE_AMOUNT);
        });

        it("fails with InsufficientFunds when shares exceed LP balance", async () => {
            const { program, pool, lendMint, authority } = setup;
            const tooManyShares = PARTICIPATE_AMOUNT + 1;

            try {
                await program.methods
                    .withdrawLent(new BN(tooManyShares))
                    .accounts({ pool, lendMint, authority: authority.publicKey })
                    .signers([authority])
                    .rpc();
                expect.fail("Expected leave with excess shares to fail with InsufficientFunds");
            } catch (err: unknown) {
                expect(err).to.be.instanceOf(Error);
                expect((err as Error).message).to.include("InsufficientFunds");
            }
        });
    });

    // ── 6. Leave queues when lend vault is drained by active borrows ────────────
    describe("leave enqueues when lend vault has insufficient liquidity", () => {
        const LEND_AMOUNT = 1_000_000;
        const COLLATERAL_DEPOSIT = 2_000_000; // more than enough for 75% LTV
        const BORROW_AMOUNT = 750_000; // 75% of 1M collateral

        let setup: Awaited<ReturnType<typeof setupTest>>;
        let userLpTokenAccount: PublicKey;

        before(async () => {
            setup = await setupTest();

            userLpTokenAccount = anchor.utils.token.associatedAddress({
                mint: setup.lpMintPda,
                owner: setup.authority.publicKey,
            });

            // Participate to fund the lend vault.
            await participateInPool(setup, LEND_AMOUNT);

            // Drain the lend vault via borrow.
            await setup.program.methods
                .depositCollateral(new anchor.BN(COLLATERAL_DEPOSIT))
                .accounts({
                    pool: setup.pool,
                    collateralMint: setup.collateralMint,
                    authority: setup.authority.publicKey,
                    userTokenAccount: setup.userCollateralTokenAccount,
                })
                .signers([setup.authority])
                .rpc();

            await setup.program.methods
                .borrow(new anchor.BN(BORROW_AMOUNT))
                .accounts({
                    pool: setup.pool,
                    lendMint: setup.lendMint,
                    authority: setup.authority.publicKey,
                })
                .signers([setup.authority])
                .rpc();
        });

        it("leave enqueues when lend vault cannot cover the redemption", async () => {
            const { program, pool, lendMint, authority } = setup;

            // LP tokens exist before leave.
            const lpBefore = await getAccount(setup.connection, userLpTokenAccount);
            expect(lpBefore.amount.toString()).to.equal(LEND_AMOUNT.toString());

            // Attempt to leave with all LP shares.
            await program.methods
                .withdrawLent(new BN(LEND_AMOUNT))
                .accounts({ pool, lendMint, authority: authority.publicKey })
                .signers([authority])
                .rpc();

            // LP tokens should be burned regardless (queued path burns on entry).
            const lpAfter = await getAccount(setup.connection, userLpTokenAccount);
            expect(lpAfter.amount.toString()).to.equal("0");

            // Lend vault still holds the borrowed amount (not drained to 0 on leave).
            const lendVault = await getAccount(setup.connection, setup.lendVaultPda);
            expect(Number(lendVault.amount)).to.equal(LEND_AMOUNT - BORROW_AMOUNT);
        });
    });
});


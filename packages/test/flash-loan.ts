import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import {
    PublicKey,
    SYSVAR_INSTRUCTIONS_PUBKEY,
    Transaction,
} from "@solana/web3.js";
import { getAccount } from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, participateInPool, TestSetup } from "./utils";

const LEND_LIQUIDITY = 500_000_000;

function flashFee(amount: number): number {
    return Math.floor((amount * 9) / 10_000);
}

async function buildFlashBorrowIx(
    setup: TestSetup,
    amount: number,
    userDestination: PublicKey
) {
    return setup.program.methods
        .flashBorrow(new BN(amount))
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            userDestination,
            sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
        })
        .instruction();
}

async function buildFlashRepayIx(
    setup: TestSetup,
    amount: number,
    userSource: PublicKey,
    authority: anchor.web3.Keypair
) {
    return setup.program.methods
        .flashRepay(new BN(amount))
        .accounts({
            pool: setup.pool,
            lendMint: setup.lendMint,
            userSource,
            authority: authority.publicKey,
            sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
        })
        .instruction();
}

describe("flash-loan", () => {
    describe("bare flash loan — happy path", () => {
        const BORROW_AMOUNT = 100_000;
        const FEE = flashFee(BORROW_AMOUNT);

        let setup: TestSetup;
        let vaultBalanceBefore: bigint;
        let userLendBalanceBefore: bigint;
        let totalLendBefore: BN;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);

            const { connection, lendVaultPda, userLendTokenAccount, program, pool } =
                setup;

            vaultBalanceBefore = (await getAccount(connection, lendVaultPda)).amount;
            userLendBalanceBefore = (
                await getAccount(connection, userLendTokenAccount)
            ).amount;
            const poolAccount = await program.account.pool.fetch(pool);
            totalLendBefore = poolAccount.totalLendDeposited;

            const borrowIx = await buildFlashBorrowIx(
                setup,
                BORROW_AMOUNT,
                userLendTokenAccount
            );
            const repayIx = await buildFlashRepayIx(
                setup,
                BORROW_AMOUNT + FEE,
                userLendTokenAccount,
                setup.authority
            );

            const tx = new Transaction().add(borrowIx, repayIx);
            await setup.provider.sendAndConfirm(tx, [setup.authority]);
        });

        it("lend vault balance increases by the fee", async () => {
            const { connection, lendVaultPda } = setup;
            const vaultAfter = (await getAccount(connection, lendVaultPda)).amount;
            expect(Number(vaultAfter - vaultBalanceBefore)).to.equal(FEE);
        });

        it("pool.total_lend_deposited increases by the fee", async () => {
            const { program, pool } = setup;
            const poolAccount = await program.account.pool.fetch(pool);
            expect(
                poolAccount.totalLendDeposited.sub(totalLendBefore).toNumber()
            ).to.equal(FEE);
        });

        it("user lend ATA balance decreases by the fee", async () => {
            const { connection, userLendTokenAccount } = setup;
            const userAfter = (
                await getAccount(connection, userLendTokenAccount)
            ).amount;
            expect(userAfter).to.equal(userLendBalanceBefore - BigInt(FEE));
        });
    });

    describe("flash loan for leverage (mock_swap pattern) — happy path", () => {
        const BORROW_AMOUNT = 100_000;
        const FEE = flashFee(BORROW_AMOUNT);

        let setup: TestSetup;
        let collateralDepositedBefore: BN;
        let vaultBalanceBefore: bigint;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);

            const {
                program,
                pool,
                collateralMint,
                lendMint,
                authority,
                userCollateralTokenAccount,
                userLendTokenAccount,
                lendVaultPda,
                connection,
            } = setup;

            const poolAccount = await program.account.pool.fetch(pool);
            collateralDepositedBefore = poolAccount.totalCollateralDeposited;
            expect(collateralDepositedBefore.toNumber()).to.equal(0);
            vaultBalanceBefore = (await getAccount(connection, lendVaultPda)).amount;

            // flash_borrow → mock_swap (lend→collateral) → deposit → flash_repay
            const borrowIx = await buildFlashBorrowIx(
                setup,
                BORROW_AMOUNT,
                userLendTokenAccount
            );

            const swapIx = await program.methods
                .mockSwap(new BN(BORROW_AMOUNT))
                .accounts({
                    mintAuthority: authority.publicKey,
                    tokenOwner: authority.publicKey,
                    mintIn: lendMint,
                    mintOut: collateralMint,
                    userTokenIn: userLendTokenAccount,
                    userTokenOut: userCollateralTokenAccount,
                })
                .instruction();

            const depositIx = await program.methods
                .depositCollateral(new BN(BORROW_AMOUNT))
                .accounts({
                    pool,
                    collateralMint,
                    authority: authority.publicKey,
                    userTokenAccount: userCollateralTokenAccount,
                })
                .instruction();

            const repayIx = await buildFlashRepayIx(
                setup,
                BORROW_AMOUNT + FEE,
                userLendTokenAccount,
                authority
            );

            const tx = new Transaction().add(borrowIx, swapIx, depositIx, repayIx);
            await setup.provider.sendAndConfirm(tx, [authority]);
        });

        it("authority collateral position equals the borrowed amount", async () => {
            const { program, userPositionPda } = setup;
            const position = await program.account.userPosition.fetch(userPositionPda);
            expect(position.collateralDeposited.toNumber()).to.equal(BORROW_AMOUNT);
        });

        it("pool.total_collateral_deposited increases by the borrowed amount", async () => {
            const { program, pool } = setup;
            const poolAccount = await program.account.pool.fetch(pool);
            expect(
                poolAccount.totalCollateralDeposited
                    .sub(collateralDepositedBefore)
                    .toNumber()
            ).to.equal(BORROW_AMOUNT);
        });

        it("lend vault balance increases by the fee", async () => {
            const { connection, lendVaultPda } = setup;
            const vaultAfter = (await getAccount(connection, lendVaultPda)).amount;
            expect(Number(vaultAfter - vaultBalanceBefore)).to.equal(FEE);
        });
    });

    describe("flash_repay without preceding flash_borrow — should fail", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
        });

        it("rejects with FlashBorrowMissing", async () => {
            const { authority, userLendTokenAccount } = setup;
            try {
                await setup.program.methods
                    .flashRepay(new BN(100_000))
                    .accounts({
                        pool: setup.pool,
                        lendMint: setup.lendMint,
                        userSource: userLendTokenAccount,
                        authority: authority.publicKey,
                        sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                    })
                    .signers([authority])
                    .rpc();
                expect.fail("should have thrown");
            } catch (e: any) {
                expect(e.message).to.include("FlashBorrowMissing");
            }
        });
    });

    describe("flash_borrow without following flash_repay — should fail", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
        });

        it("rejects with FlashRepayMissing", async () => {
            const { userLendTokenAccount } = setup;
            try {
                await setup.program.methods
                    .flashBorrow(new BN(100_000))
                    .accounts({
                        pool: setup.pool,
                        lendMint: setup.lendMint,
                        userDestination: userLendTokenAccount,
                        sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                    })
                    .rpc();
                expect.fail("should have thrown");
            } catch (e: any) {
                expect(e.message).to.include("FlashRepayMissing");
            }
        });
    });

    describe("repay amount too small — fee not covered", () => {
        let setup: TestSetup;

        before(async () => {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);
        });

        it("rejects when repay equals principal with no fee", async () => {
            const BORROW_AMOUNT = 100_000;
            const { authority, userLendTokenAccount } = setup;

            const borrowIx = await buildFlashBorrowIx(
                setup,
                BORROW_AMOUNT,
                userLendTokenAccount
            );
            // Repay exactly the principal — fee not included, so flash_borrow should reject.
            const repayIx = await buildFlashRepayIx(
                setup,
                BORROW_AMOUNT,
                userLendTokenAccount,
                authority
            );

            try {
                const tx = new Transaction().add(borrowIx, repayIx);
                await setup.provider.sendAndConfirm(tx, [authority]);
                expect.fail("should have thrown");
            } catch (e: any) {
                const msg: string = e?.toString() ?? "";
                const code: string = e?.error?.errorCode?.code ?? "";
                expect(
                    msg.includes("FlashRepayMissing") ||
                    msg.includes("FlashLoanFeeNotCovered") ||
                    code === "FlashRepayMissing" ||
                    code === "FlashLoanFeeNotCovered"
                ).to.be.true;
            }
        });
    });
});

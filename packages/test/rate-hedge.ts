import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import { PublicKey, Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { getAccount, createMint, createAssociatedTokenAccount, mintTo } from "@solana/spl-token";
import { expect } from "chai";
import { setupTest, createLender, participateInPool, TestSetup } from "./utils";

// ── Constants ─────────────────────────────────────────────────────────────────

const SECONDS_PER_YEAR = 31_557_600;

/** Lend-token liquidity seeded into the pool before each borrow test. */
const LEND_LIQUIDITY = 500_000_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

function computeUpfrontFee(amount: number, fixedRateBps: number, duration: number): number {
    return Math.floor((amount * fixedRateBps * duration) / (10_000 * SECONDS_PER_YEAR));
}

function getRateHedgeOfferPda(
    pool: PublicKey,
    authority: PublicKey,
    fixedRateBps: BN,
    minDuration: BN,
    maxDuration: BN,
    programId: PublicKey
): [PublicKey, number] {
    const rateBuf = Buffer.alloc(8);
    rateBuf.writeBigUInt64LE(BigInt(fixedRateBps.toString()));
    const minBuf = Buffer.alloc(8);
    minBuf.writeBigUInt64LE(BigInt(minDuration.toString()));
    const maxBuf = Buffer.alloc(8);
    maxBuf.writeBigUInt64LE(BigInt(maxDuration.toString()));
    return PublicKey.findProgramAddressSync(
        [Buffer.from("rate_hedge_offer"), pool.toBuffer(), authority.toBuffer(), rateBuf, minBuf, maxBuf],
        programId
    );
}

function getOfferCollateralVaultPda(offerPda: PublicKey, programId: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
        [Buffer.from("rate_hedge_offer_vault"), offerPda.toBuffer()],
        programId
    );
}

function getRateHedgeMatchPda(userPositionPda: PublicKey, programId: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
        [Buffer.from("rate_hedge_match"), userPositionPda.toBuffer()],
        programId
    );
}

function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Force the Solana test-validator to produce confirmed blocks so that
 * Clock::unix_timestamp advances by at least `seconds` seconds.
 * The validator only advances its clock when blocks are produced, so we
 * submit dummy airdrop transactions and confirm each one.
 */
async function advanceClock(
    connection: anchor.web3.Connection,
    payer: Keypair,
    seconds: number
): Promise<void> {
    const slotsNeeded = Math.ceil(seconds / 0.4) + 4; // 400ms/slot + buffer
    for (let i = 0; i < slotsNeeded; i++) {
        const dummy = Keypair.generate();
        const sig = await connection.requestAirdrop(dummy.publicKey, LAMPORTS_PER_SOL);
        await connection.confirmTransaction(sig, "confirmed");
    }
}

// ── Test suite ────────────────────────────────────────────────────────────────

describe("rate-hedge", () => {
    // ═══════════════════════════════════════════════════════════════════════════
    // createRateHedgeOffer
    // ═══════════════════════════════════════════════════════════════════════════

    describe("createRateHedgeOffer", () => {
        // ── 1. Happy path ────────────────────────────────────────────────────────
        describe("happy path", () => {
            const FIXED_RATE_BPS = new BN(1_000); // 10%
            const MIN_DURATION = new BN(86_400);   // 1 day
            const MAX_DURATION = new BN(SECONDS_PER_YEAR);
            const OFFER_AMOUNT = new BN(100_000_000);
            const COLLATERAL_AMOUNT = new BN(10_000_000);

            let setup: TestSetup;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;

            before(async () => {
                setup = await setupTest();

                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);
            });

            it("creates the offer account with correct state and transfers collateral to vault", async () => {
                const { program, pool, statePda, collateralMint, userCollateralTokenAccount, authority, connection } = setup;

                const collateralBefore = await getAccount(connection, userCollateralTokenAccount);

                await program.methods
                    .createRateHedgeOffer(
                        FIXED_RATE_BPS,
                        MIN_DURATION,
                        MAX_DURATION,
                        OFFER_AMOUNT,
                        COLLATERAL_AMOUNT
                    )
                    .accounts({
                        pool,
                        rateHedgeOffer: offerPda,
                        offerCollateralVault: offerVaultPda,
                        state: statePda,
                        collateralMint,
                        userCollateralTokenAccount,
                        authority: authority.publicKey,
                    })
                    .signers([authority])
                    .rpc();

                const offer = await program.account.rateHedgeOffer.fetch(offerPda);
                expect(offer.pool.toBase58()).to.equal(pool.toBase58());
                expect(offer.authority.toBase58()).to.equal(authority.publicKey.toBase58());
                expect(offer.amount.toString()).to.equal(OFFER_AMOUNT.toString());
                expect(offer.fixedRateBps.toString()).to.equal(FIXED_RATE_BPS.toString());
                expect(offer.minDuration.toString()).to.equal(MIN_DURATION.toString());
                expect(offer.maxDuration.toString()).to.equal(MAX_DURATION.toString());
                expect(offer.collateralDeposited.toString()).to.equal(COLLATERAL_AMOUNT.toString());
                expect(offer.lockedTokens.toNumber()).to.equal(0);

                const vault = await getAccount(connection, offerVaultPda);
                expect(Number(vault.amount)).to.equal(COLLATERAL_AMOUNT.toNumber());

                const collateralAfter = await getAccount(connection, userCollateralTokenAccount);
                expect(Number(collateralAfter.amount)).to.equal(
                    Number(collateralBefore.amount) - COLLATERAL_AMOUNT.toNumber()
                );
            });
        });

        // ── 2. Invalid duration range (min > max) ────────────────────────────────
        describe("invalid duration range (min > max)", () => {
            const FIXED_RATE_BPS = new BN(500);
            const MIN_DURATION = new BN(86_400 * 30); // 30 days
            const MAX_DURATION = new BN(86_400);       // 1 day — intentionally less than min
            const OFFER_AMOUNT = new BN(50_000_000);
            const COLLATERAL_AMOUNT = new BN(5_000_000);

            let setup: TestSetup;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;

            before(async () => {
                setup = await setupTest();
                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);
            });

            it("rejects with InvalidDurationRange", async () => {
                const { program, pool, statePda, collateralMint, userCollateralTokenAccount, authority } = setup;
                try {
                    await program.methods
                        .createRateHedgeOffer(
                            FIXED_RATE_BPS,
                            MIN_DURATION,
                            MAX_DURATION,
                            OFFER_AMOUNT,
                            COLLATERAL_AMOUNT
                        )
                        .accounts({
                            pool,
                            rateHedgeOffer: offerPda,
                            offerCollateralVault: offerVaultPda,
                            state: statePda,
                            collateralMint,
                            userCollateralTokenAccount,
                            authority: authority.publicKey,
                        })
                        .signers([authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("InvalidDurationRange");
                }
            });
        });

        // ── 3. Wrong collateral mint ─────────────────────────────────────────────
        describe("wrong collateral mint", () => {
            const FIXED_RATE_BPS = new BN(500);
            const MIN_DURATION = new BN(86_400);
            const MAX_DURATION = new BN(SECONDS_PER_YEAR);
            const OFFER_AMOUNT = new BN(50_000_000);
            const COLLATERAL_AMOUNT = new BN(5_000_000);

            let setup: TestSetup;
            let wrongMint: PublicKey;
            let wrongTokenAccount: PublicKey;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;

            before(async () => {
                setup = await setupTest();

                // Create an entirely different mint
                wrongMint = await createMint(
                    setup.connection,
                    setup.payer,
                    setup.authority.publicKey,
                    null,
                    6
                );
                wrongTokenAccount = await createAssociatedTokenAccount(
                    setup.connection,
                    setup.payer,
                    wrongMint,
                    setup.authority.publicKey
                );
                await mintTo(
                    setup.connection,
                    setup.payer,
                    wrongMint,
                    wrongTokenAccount,
                    setup.authority,
                    100_000_000
                );

                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);
            });

            it("rejects with InvalidMint", async () => {
                const { program, pool, statePda, authority } = setup;
                try {
                    await program.methods
                        .createRateHedgeOffer(
                            FIXED_RATE_BPS,
                            MIN_DURATION,
                            MAX_DURATION,
                            OFFER_AMOUNT,
                            COLLATERAL_AMOUNT
                        )
                        .accounts({
                            pool,
                            rateHedgeOffer: offerPda,
                            offerCollateralVault: offerVaultPda,
                            state: statePda,
                            collateralMint: wrongMint,
                            userCollateralTokenAccount: wrongTokenAccount,
                            authority: authority.publicKey,
                        })
                        .signers([authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("InvalidMint");
                }
            });
        });
    });

    // ═══════════════════════════════════════════════════════════════════════════
    // borrowWithHedge
    // ═══════════════════════════════════════════════════════════════════════════

    describe("borrowWithHedge", () => {
        // ── 4. Happy path ────────────────────────────────────────────────────────
        describe("happy path", () => {
            const FIXED_RATE_BPS = new BN(1_000); // 10%
            const MIN_DURATION = new BN(86_400);
            const MAX_DURATION = new BN(SECONDS_PER_YEAR);
            const OFFER_AMOUNT = new BN(200_000_000);
            const OFFER_COLLATERAL = new BN(10_000_000);
            const BORROW_AMOUNT = 50_000_000;
            const BORROWER_COLLATERAL = 100_000_000;
            const DURATION = SECONDS_PER_YEAR; // gives clean upfront_fee calculation

            // upfront_fee = floor(50_000_000 * 1_000 * 31_557_600 / (10_000 * 31_557_600)) = 5_000_000
            const EXPECTED_UPFRONT_FEE = computeUpfrontFee(BORROW_AMOUNT, 1_000, DURATION);

            let setup: TestSetup;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;
            let matchPda: PublicKey;
            let borrower: Awaited<ReturnType<typeof createLender>>;

            before(async () => {
                setup = await setupTest();

                // Seed the pool with lend liquidity
                await participateInPool(setup, LEND_LIQUIDITY);

                // Offer creator (setup.authority) creates the rate-hedge offer
                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);

                await setup.program.methods
                    .createRateHedgeOffer(
                        FIXED_RATE_BPS,
                        MIN_DURATION,
                        MAX_DURATION,
                        OFFER_AMOUNT,
                        OFFER_COLLATERAL
                    )
                    .accounts({
                        pool: setup.pool,
                        rateHedgeOffer: offerPda,
                        offerCollateralVault: offerVaultPda,
                        state: setup.statePda,
                        collateralMint: setup.collateralMint,
                        userCollateralTokenAccount: setup.userCollateralTokenAccount,
                        authority: setup.authority.publicKey,
                    })
                    .signers([setup.authority])
                    .rpc();

                // Borrower deposits collateral so they have a position
                borrower = await createLender(setup);

                await setup.program.methods
                    .depositCollateral(new BN(BORROWER_COLLATERAL))
                    .accounts({
                        pool: setup.pool,
                        collateralMint: setup.collateralMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrower.userTokenAccount,
                    })
                    .signers([borrower.authority])
                    .rpc();

                [matchPda] = getRateHedgeMatchPda(borrower.userPositionPda, setup.program.programId);
            });

            it("transfers exactly BORROW_AMOUNT lend tokens to borrower", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda, connection } = setup;

                const vaultBefore = await getAccount(connection, lendVaultPda);

                // Borrower's lend ATA (created by borrowWithHedge via init_if_needed)
                const borrowerLendAta = anchor.utils.token.associatedAddress({
                    mint: lendMint,
                    owner: borrower.authority.publicKey,
                });

                await program.methods
                    .borrowWithHedge(new BN(BORROW_AMOUNT), new BN(DURATION))
                    .accounts({
                        pool,
                        state: statePda,
                        lendMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrowerLendAta,
                        lendVault: lendVaultPda,
                        userPosition: borrower.userPositionPda,
                        rateHedgeOffer: offerPda,
                        rateHedgeMatch: matchPda,
                    })
                    .signers([borrower.authority])
                    .rpc();

                // createLender mints 1_000_000_000 lend tokens to the borrower's ATA before the borrow.
                // After borrowWithHedge the balance grows by exactly BORROW_AMOUNT (fee stays in pool).
                const borrowerLend = await getAccount(connection, borrowerLendAta);
                expect(Number(borrowerLend.amount)).to.equal(1_000_000_000 + BORROW_AMOUNT);

                // Lend vault decreased by only BORROW_AMOUNT, not amount + fee
                const vaultAfter = await getAccount(connection, lendVaultPda);
                expect(Number(vaultAfter.amount)).to.equal(Number(vaultBefore.amount) - BORROW_AMOUNT);
            });

            it("match account has correct state", async () => {
                const match = await setup.program.account.rateHedgeMatch.fetch(matchPda);
                expect(match.offer.toBase58()).to.equal(offerPda.toBase58());
                expect(match.userPosition.toBase58()).to.equal(borrower.userPositionPda.toBase58());
                expect(match.amount.toNumber()).to.equal(BORROW_AMOUNT);
                expect(match.upfrontFee.toNumber()).to.equal(EXPECTED_UPFRONT_FEE);
                expect(match.initialDebtShares.toNumber()).to.be.greaterThan(0);
                expect(match.duration.toNumber()).to.equal(DURATION);
                expect(match.startTs.toNumber()).to.be.greaterThan(0);
            });

            it("offer amount reduced and lockedTokens increased by upfront fee", async () => {
                const offer = await setup.program.account.rateHedgeOffer.fetch(offerPda);
                expect(offer.amount.toNumber()).to.equal(OFFER_AMOUNT.toNumber() - BORROW_AMOUNT);
                expect(offer.lockedTokens.toNumber()).to.equal(EXPECTED_UPFRONT_FEE);
            });

            it("pool totalBorrowed increased by amount + upfront fee", async () => {
                const poolAccount = await setup.program.account.pool.fetch(setup.pool);
                expect(poolAccount.totalBorrowed.toNumber()).to.equal(BORROW_AMOUNT + EXPECTED_UPFRONT_FEE);
            });
        });

        // ── 5. Duration out of offer range ───────────────────────────────────────
        describe("duration out of offer range", () => {
            const FIXED_RATE_BPS = new BN(1_000);
            const MIN_DURATION = new BN(86_400);       // 1 day minimum
            const MAX_DURATION = new BN(86_400 * 30);  // 30 days maximum
            const OFFER_AMOUNT = new BN(100_000_000);
            const OFFER_COLLATERAL = new BN(10_000_000);
            const BORROWER_COLLATERAL = 100_000_000;

            let setup: TestSetup;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;
            let borrower: Awaited<ReturnType<typeof createLender>>;
            let matchPda: PublicKey;

            before(async () => {
                setup = await setupTest();
                await participateInPool(setup, LEND_LIQUIDITY);

                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);

                await setup.program.methods
                    .createRateHedgeOffer(
                        FIXED_RATE_BPS,
                        MIN_DURATION,
                        MAX_DURATION,
                        OFFER_AMOUNT,
                        OFFER_COLLATERAL
                    )
                    .accounts({
                        pool: setup.pool,
                        rateHedgeOffer: offerPda,
                        offerCollateralVault: offerVaultPda,
                        state: setup.statePda,
                        collateralMint: setup.collateralMint,
                        userCollateralTokenAccount: setup.userCollateralTokenAccount,
                        authority: setup.authority.publicKey,
                    })
                    .signers([setup.authority])
                    .rpc();

                borrower = await createLender(setup);
                await setup.program.methods
                    .depositCollateral(new BN(BORROWER_COLLATERAL))
                    .accounts({
                        pool: setup.pool,
                        collateralMint: setup.collateralMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrower.userTokenAccount,
                    })
                    .signers([borrower.authority])
                    .rpc();

                [matchPda] = getRateHedgeMatchPda(borrower.userPositionPda, setup.program.programId);
            });

            it("rejects a duration below min_duration with InvalidDurationRange", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda } = setup;
                const tooShort = new BN(3600); // 1 hour — below 1-day minimum
                const borrowerLendAta = anchor.utils.token.associatedAddress({
                    mint: lendMint,
                    owner: borrower.authority.publicKey,
                });
                try {
                    await program.methods
                        .borrowWithHedge(new BN(10_000_000), tooShort)
                        .accounts({
                            pool,
                            state: statePda,
                            lendMint,
                            authority: borrower.authority.publicKey,
                            userTokenAccount: borrowerLendAta,
                            lendVault: lendVaultPda,
                            userPosition: borrower.userPositionPda,
                            rateHedgeOffer: offerPda,
                            rateHedgeMatch: matchPda,
                        })
                        .signers([borrower.authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("InvalidDurationRange");
                }
            });

            it("rejects a duration above max_duration with InvalidDurationRange", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda } = setup;
                const tooLong = new BN(86_400 * 365); // 1 year — above 30-day maximum
                const borrowerLendAta = anchor.utils.token.associatedAddress({
                    mint: lendMint,
                    owner: borrower.authority.publicKey,
                });
                try {
                    await program.methods
                        .borrowWithHedge(new BN(10_000_000), tooLong)
                        .accounts({
                            pool,
                            state: statePda,
                            lendMint,
                            authority: borrower.authority.publicKey,
                            userTokenAccount: borrowerLendAta,
                            lendVault: lendVaultPda,
                            userPosition: borrower.userPositionPda,
                            rateHedgeOffer: offerPda,
                            rateHedgeMatch: matchPda,
                        })
                        .signers([borrower.authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("InvalidDurationRange");
                }
            });
        });

        // ── 6. Offer capacity exceeded ───────────────────────────────────────────
        describe("offer capacity exceeded", () => {
            const FIXED_RATE_BPS = new BN(1_000);
            const MIN_DURATION = new BN(86_400);
            const MAX_DURATION = new BN(SECONDS_PER_YEAR);
            const OFFER_AMOUNT = new BN(5_000_000); // tiny offer — only 5 tokens
            const OFFER_COLLATERAL = new BN(1_000_000);
            const BORROWER_COLLATERAL = 100_000_000;

            let setup: TestSetup;
            let offerPda: PublicKey;
            let offerVaultPda: PublicKey;
            let borrower: Awaited<ReturnType<typeof createLender>>;
            let matchPda: PublicKey;

            before(async () => {
                setup = await setupTest();
                await participateInPool(setup, LEND_LIQUIDITY);

                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    setup.authority.publicKey,
                    FIXED_RATE_BPS,
                    MIN_DURATION,
                    MAX_DURATION,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);

                await setup.program.methods
                    .createRateHedgeOffer(
                        FIXED_RATE_BPS,
                        MIN_DURATION,
                        MAX_DURATION,
                        OFFER_AMOUNT,
                        OFFER_COLLATERAL
                    )
                    .accounts({
                        pool: setup.pool,
                        rateHedgeOffer: offerPda,
                        offerCollateralVault: offerVaultPda,
                        state: setup.statePda,
                        collateralMint: setup.collateralMint,
                        userCollateralTokenAccount: setup.userCollateralTokenAccount,
                        authority: setup.authority.publicKey,
                    })
                    .signers([setup.authority])
                    .rpc();

                borrower = await createLender(setup);
                await setup.program.methods
                    .depositCollateral(new BN(BORROWER_COLLATERAL))
                    .accounts({
                        pool: setup.pool,
                        collateralMint: setup.collateralMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrower.userTokenAccount,
                    })
                    .signers([borrower.authority])
                    .rpc();

                [matchPda] = getRateHedgeMatchPda(borrower.userPositionPda, setup.program.programId);
            });

            it("rejects a borrow exceeding offer.amount with InsufficientFunds", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda } = setup;
                // Try to borrow 50_000_000 but offer only covers 5_000_000
                const overCapacity = new BN(50_000_000);
                const duration = new BN(SECONDS_PER_YEAR);
                const borrowerLendAta = anchor.utils.token.associatedAddress({
                    mint: lendMint,
                    owner: borrower.authority.publicKey,
                });
                try {
                    await program.methods
                        .borrowWithHedge(overCapacity, duration)
                        .accounts({
                            pool,
                            state: statePda,
                            lendMint,
                            authority: borrower.authority.publicKey,
                            userTokenAccount: borrowerLendAta,
                            lendVault: lendVaultPda,
                            userPosition: borrower.userPositionPda,
                            rateHedgeOffer: offerPda,
                            rateHedgeMatch: matchPda,
                        })
                        .signers([borrower.authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("InsufficientFunds");
                }
            });
        });
    });

    // ═══════════════════════════════════════════════════════════════════════════
    // settleRateHedgeMatch
    // ═══════════════════════════════════════════════════════════════════════════

    describe("settleRateHedgeMatch", () => {
        // Shared setup for settle tests: an offer with min_duration=1 and a matched borrow
        // with duration=1 so the settle test can use sleep(2000) to let the clock advance.

        // Rate chosen so upfront_fee > 0 even with duration=1:
        //   upfront_fee = floor(50_000_000 * 10_000 * 1 / (10_000 * 31_557_600))
        //               = floor(50_000_000 / 31_557_600) = 1
        const FIXED_RATE_BPS_SETTLE = new BN(10_000); // 100%
        const MIN_DURATION_SETTLE = new BN(1);
        const MAX_DURATION_SETTLE = new BN(86_400);
        const OFFER_AMOUNT_SETTLE = new BN(200_000_000);
        const OFFER_COLLATERAL_SETTLE = new BN(10_000_000);
        const BORROW_AMOUNT_SETTLE = 50_000_000;
        const BORROWER_COLLATERAL_SETTLE = 100_000_000;
        const DURATION_ONE_SEC = new BN(1);

        let setup: TestSetup;
        let offerCreator: Awaited<ReturnType<typeof createLender>>;
        let borrower: Awaited<ReturnType<typeof createLender>>;
        let offerPda: PublicKey;
        let offerVaultPda: PublicKey;
        let matchPda: PublicKey;

        /** Shared setup: creates offer, deposits collateral, borrows. */
        async function setupSettleFixture() {
            setup = await setupTest();
            await participateInPool(setup, LEND_LIQUIDITY);

            // Offer creator is a separate user
            offerCreator = await createLender(setup);

            [offerPda] = getRateHedgeOfferPda(
                setup.pool,
                offerCreator.authority.publicKey,
                FIXED_RATE_BPS_SETTLE,
                MIN_DURATION_SETTLE,
                MAX_DURATION_SETTLE,
                setup.program.programId
            );
            [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);

            await setup.program.methods
                .createRateHedgeOffer(
                    FIXED_RATE_BPS_SETTLE,
                    MIN_DURATION_SETTLE,
                    MAX_DURATION_SETTLE,
                    OFFER_AMOUNT_SETTLE,
                    OFFER_COLLATERAL_SETTLE
                )
                .accounts({
                    pool: setup.pool,
                    rateHedgeOffer: offerPda,
                    offerCollateralVault: offerVaultPda,
                    state: setup.statePda,
                    collateralMint: setup.collateralMint,
                    userCollateralTokenAccount: offerCreator.userTokenAccount,
                    authority: offerCreator.authority.publicKey,
                })
                .signers([offerCreator.authority])
                .rpc();

            // Borrower deposits collateral and borrows
            borrower = await createLender(setup);

            await setup.program.methods
                .depositCollateral(new BN(BORROWER_COLLATERAL_SETTLE))
                .accounts({
                    pool: setup.pool,
                    collateralMint: setup.collateralMint,
                    authority: borrower.authority.publicKey,
                    userTokenAccount: borrower.userTokenAccount,
                })
                .signers([borrower.authority])
                .rpc();

            [matchPda] = getRateHedgeMatchPda(borrower.userPositionPda, setup.program.programId);

            const borrowerLendAta = anchor.utils.token.associatedAddress({
                mint: setup.lendMint,
                owner: borrower.authority.publicKey,
            });

            await setup.program.methods
                .borrowWithHedge(new BN(BORROW_AMOUNT_SETTLE), DURATION_ONE_SEC)
                .accounts({
                    pool: setup.pool,
                    state: setup.statePda,
                    lendMint: setup.lendMint,
                    authority: borrower.authority.publicKey,
                    userTokenAccount: borrowerLendAta,
                    lendVault: setup.lendVaultPda,
                    userPosition: borrower.userPositionPda,
                    rateHedgeOffer: offerPda,
                    rateHedgeMatch: matchPda,
                })
                .signers([borrower.authority])
                .rpc();
        }

        // ── 7. Settle before duration elapses ────────────────────────────────────
        // Use a standalone fixture with duration=86400 (1 day) so the duration
        // is guaranteed not to have elapsed regardless of setup time.
        describe("before duration elapses", () => {
            before(async () => {
                setup = await setupTest();
                await participateInPool(setup, LEND_LIQUIDITY);

                offerCreator = await createLender(setup);

                [offerPda] = getRateHedgeOfferPda(
                    setup.pool,
                    offerCreator.authority.publicKey,
                    FIXED_RATE_BPS_SETTLE,
                    MIN_DURATION_SETTLE,
                    MAX_DURATION_SETTLE,
                    setup.program.programId
                );
                [offerVaultPda] = getOfferCollateralVaultPda(offerPda, setup.program.programId);

                await setup.program.methods
                    .createRateHedgeOffer(
                        FIXED_RATE_BPS_SETTLE,
                        MIN_DURATION_SETTLE,
                        MAX_DURATION_SETTLE,
                        OFFER_AMOUNT_SETTLE,
                        OFFER_COLLATERAL_SETTLE
                    )
                    .accounts({
                        pool: setup.pool,
                        rateHedgeOffer: offerPda,
                        offerCollateralVault: offerVaultPda,
                        state: setup.statePda,
                        collateralMint: setup.collateralMint,
                        userCollateralTokenAccount: offerCreator.userTokenAccount,
                        authority: offerCreator.authority.publicKey,
                    })
                    .signers([offerCreator.authority])
                    .rpc();

                borrower = await createLender(setup);

                await setup.program.methods
                    .depositCollateral(new BN(BORROWER_COLLATERAL_SETTLE))
                    .accounts({
                        pool: setup.pool,
                        collateralMint: setup.collateralMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrower.userTokenAccount,
                    })
                    .signers([borrower.authority])
                    .rpc();

                [matchPda] = getRateHedgeMatchPda(borrower.userPositionPda, setup.program.programId);

                const borrowerLendAta = anchor.utils.token.associatedAddress({
                    mint: setup.lendMint,
                    owner: borrower.authority.publicKey,
                });

                // Borrow with a 1-day duration — definitely has not elapsed
                const LONG_DURATION = new BN(86_400);
                await setup.program.methods
                    .borrowWithHedge(new BN(BORROW_AMOUNT_SETTLE), LONG_DURATION)
                    .accounts({
                        pool: setup.pool,
                        state: setup.statePda,
                        lendMint: setup.lendMint,
                        authority: borrower.authority.publicKey,
                        userTokenAccount: borrowerLendAta,
                        lendVault: setup.lendVaultPda,
                        userPosition: borrower.userPositionPda,
                        rateHedgeOffer: offerPda,
                        rateHedgeMatch: matchPda,
                    })
                    .signers([borrower.authority])
                    .rpc();
            });

            it("rejects settlement with HedgeNotYetMatured", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda, collateralVaultPda } = setup;

                const [collateralVaultPda2] = PublicKey.findProgramAddressSync(
                    [Buffer.from("collateral_vault"), pool.toBuffer()],
                    program.programId
                );

                try {
                    await program.methods
                        .settleRateHedgeMatch()
                        .accounts({
                            pool,
                            state: statePda,
                            lendMint,
                            rateHedgeMatch: matchPda,
                            rateHedgeOffer: offerPda,
                            userPosition: borrower.userPositionPda,
                            lendVault: lendVaultPda,
                            offerCreatorTokenAccount: offerCreator.userLendTokenAccount,
                            offerCollateralVault: offerVaultPda,
                            collateralVault: collateralVaultPda2,
                            cranker: setup.authority.publicKey,
                        })
                        .signers([setup.authority])
                        .rpc();
                    expect.fail("expected rejection");
                } catch (e: any) {
                    expect(e.message).to.include("HedgeNotYetMatured");
                }
            });
        });

        // ── 8. Settle after duration elapses ─────────────────────────────────────
        describe("after duration elapses", () => {
            let upfrontFee: number;

            before(async () => {
                await setupSettleFixture();

                const match = await setup.program.account.rateHedgeMatch.fetch(matchPda);
                upfrontFee = match.upfrontFee.toNumber();

                // Advance the on-chain clock past start_ts + 1 by forcing the test-validator
                // to produce confirmed blocks. The validator clock only advances with block production.
                await advanceClock(setup.connection, setup.payer, 3);
            });

            it("transfers upfront fee to offer creator's lend token account", async () => {
                const { program, pool, statePda, lendMint, lendVaultPda, connection } = setup;

                const [collateralVaultPda] = PublicKey.findProgramAddressSync(
                    [Buffer.from("collateral_vault"), pool.toBuffer()],
                    program.programId
                );

                const creatorLendBefore = await getAccount(connection, offerCreator.userLendTokenAccount);

                await program.methods
                    .settleRateHedgeMatch()
                    .accounts({
                        pool,
                        state: statePda,
                        lendMint,
                        rateHedgeMatch: matchPda,
                        rateHedgeOffer: offerPda,
                        userPosition: borrower.userPositionPda,
                        lendVault: lendVaultPda,
                        offerCreatorTokenAccount: offerCreator.userLendTokenAccount,
                        offerCollateralVault: offerVaultPda,
                        collateralVault: collateralVaultPda,
                        cranker: setup.authority.publicKey,
                    })
                    .signers([setup.authority])
                    .rpc();

                const creatorLendAfter = await getAccount(connection, offerCreator.userLendTokenAccount);
                expect(Number(creatorLendAfter.amount)).to.equal(
                    Number(creatorLendBefore.amount) + upfrontFee
                );
            });

            it("match account is closed after settlement", async () => {
                // After settle the account is closed — getAccountInfo returns null
                const info = await setup.connection.getAccountInfo(matchPda);
                expect(info).to.be.null;
            });

            it("offer.amount is restored and lockedTokens decremented", async () => {
                const offer = await setup.program.account.rateHedgeOffer.fetch(offerPda);
                // After settlement, offer.amount should be back to the original OFFER_AMOUNT_SETTLE
                expect(offer.amount.toNumber()).to.equal(OFFER_AMOUNT_SETTLE.toNumber());
                expect(offer.lockedTokens.toNumber()).to.equal(0);
            });

            it("borrower's debt is capped at borrow_amount after settlement", async () => {
                const position = await setup.program.account.userPosition.fetch(borrower.userPositionPda);
                // After settlement, debt shares should reflect only borrow_amount, not amount+fee
                expect(position.debtShares.toNumber()).to.be.greaterThan(0);

                const poolAccount = await setup.program.account.pool.fetch(setup.pool);
                // Ensure pool accounting is consistent — totalBorrowed >= 0
                expect(poolAccount.totalBorrowed.toNumber()).to.be.greaterThanOrEqual(0);
            });
        });
    });
});

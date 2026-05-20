import * as anchor from "@anchor-lang/core";
import { BN } from "@anchor-lang/core";
import { expect } from "chai";
import { setupTest, participateInPool, TestSetup } from "./utils";
import { Keypair, PublicKey, SYSVAR_INSTRUCTIONS_PUBKEY, Transaction } from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";



/** 9 bps flash fee — mirrors the on-chain constant. */
function computeFlashFee(amount: BN): BN {
  return amount.muln(9).divn(10_000);
}

/**
 * Open a leveraged (multiply) position via a flash-loan loop.
 *
 * Transaction sequence:
 *   1. depositCollateral(amount)              — user's own capital
 *   2. flashBorrow(extra = amount × (L−1))    — lend tokens
 *   3. mockSwap(extra, lend → collateral)     — synthetic 1:1 swap
 *   4. depositCollateral(extra)               — swapped collateral
 *   5. borrow(extra + fee)                    — lend tokens to cover flash repay
 *   6. flashRepay(extra + fee)
 */
async function openMultiply(
  setup: TestSetup,
  authority: Keypair,
  amountRaw: BN,
  leverage: number
): Promise<void> {
  const { program, pool, collateralMint, lendMint, userCollateralTokenAccount, userLendTokenAccount } = setup;

  const userCollateralAta = userCollateralTokenAccount;
  const userLendAta = userLendTokenAccount;

  // extra = amount × (leverage − 1)
  const leverageMilli = Math.round(leverage * 1_000);
  const extraRaw = amountRaw.muln(leverageMilli - 1_000).divn(1_000);
  const flashFee = computeFlashFee(extraRaw);
  const flashRepayAmt = extraRaw.add(flashFee);

  const [depositInitialIx, flashBorrowIx, swapIx, depositExtraIx, borrowIx, flashRepayIx] =
    await Promise.all([
      program.methods
        .depositCollateral(amountRaw)
        .accounts({ pool, collateralMint, authority: authority.publicKey, userTokenAccount: userCollateralAta })
        .signers([authority])
        .instruction(),
      program.methods
        .flashBorrow(extraRaw)
        .accounts({
          pool,
          lendMint,
          userDestination: userLendAta,
          sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
        })
        .instruction(),
      program.methods
        .mockSwap(extraRaw)
        .accounts({
          mintAuthority: authority.publicKey,
          tokenOwner: authority.publicKey,
          mintIn: lendMint,
          mintOut: collateralMint,
          userTokenIn: userLendAta,
          userTokenOut: userCollateralAta,
        })
        .signers([authority])
        .instruction(),
      program.methods
        .depositCollateral(extraRaw)
        .accounts({ pool, collateralMint, authority: authority.publicKey, userTokenAccount: userCollateralAta })
        .signers([authority])
        .instruction(),
      program.methods
        .borrow(flashRepayAmt)
        .accounts({ pool, lendMint, authority: authority.publicKey })
        .signers([authority])
        .instruction(),
      program.methods
        .flashRepay(flashRepayAmt)
        .accounts({
          pool,
          lendMint,
          userSource: userLendAta,
          authority: authority.publicKey,
          sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
        })
        .signers([authority])
        .instruction(),
    ]);

  const tx = new Transaction().add(
    depositInitialIx,
    flashBorrowIx,
    swapIx,
    depositExtraIx,
    borrowIx,
    flashRepayIx,
  );

  const { blockhash } = await setup.connection.getLatestBlockhash();
  tx.recentBlockhash = blockhash;

  // Sign and send the transaction
  // The provider's wallet will sign for the fee payer and any other required signers
  await setup.provider.sendAndConfirm(tx, [authority]);
}

describe("multiply (leverage)", () => {
  // High LTV pool: 97% LTV allows for ~33x max leverage (1 / (1 - 0.97))
  const HIGH_LTV = 97;
  const LEVERAGE_30X = 30;

  describe("30x leverage with 97% LTV pool", () => {
    const INITIAL_COLLATERAL = 1_000_000; // 1 token (6 decimals)
    let setup: TestSetup;

    before(async () => {
      // Create pool with 97% LTV - setupTest already mints 1000 tokens to authority
      setup = await setupTest(undefined, HIGH_LTV);

      // Add liquidity to the pool
      await participateInPool(setup, 500_000_000); // 500 tokens liquidity
    });

    it("opens a 30x leveraged position", async () => {
      const { program, pool, userPositionPda, authority } = setup;

      const initialCollateralRaw = new BN(INITIAL_COLLATERAL);

      // Open 30x leverage position
      await openMultiply(setup, authority, initialCollateralRaw, LEVERAGE_30X);

      // Verify position state
      const position = await program.account.userPosition.fetch(userPositionPda);

      // Total collateral should be ~30x initial (initial + 29x from swap)
      // 1 tokens * 30 = 30 tokens collateral deposited
      const expectedTotalCollateral = INITIAL_COLLATERAL * LEVERAGE_30X;
      expect(Number(position.collateralDeposited)).to.be.closeTo(
        expectedTotalCollateral,
        1000 // Small tolerance for rounding
      );

      // Should have debt shares (borrowed to repay flash loan)
      expect(position.debtShares.toNumber()).to.be.greaterThan(0);

      console.log("30x leverage position opened successfully!");
      console.log(`  Initial collateral: ${INITIAL_COLLATERAL / 1e6} tokens`);
      console.log(`  Total collateral deposited: ${Number(position.collateralDeposited) / 1e6} tokens`);
      console.log(`  Debt shares: ${position.debtShares.toNumber()}`);
    });

    it("calculates correct leverage ratio", async () => {
      const { program, userPositionPda } = setup;

      const position = await program.account.userPosition.fetch(userPositionPda);
      const poolAccount = await program.account.pool.fetch(setup.pool);

      // Calculate actual leverage: Total Collateral / (Total Collateral - Debt)
      // Debt = debt_shares * total_borrowed / total_debt_shares
      const debtShares = BigInt(position.debtShares.toString());
      const totalBorrowed = BigInt(poolAccount.totalBorrowed.toString());
      const totalDebtShares = BigInt(poolAccount.totalDebtShares.toString());

      const debtAmount = Number((debtShares * totalBorrowed) / totalDebtShares);
      const collateralAmount = Number(position.collateralDeposited);

      // Leverage = Collateral / (Collateral - Debt) = Collateral / InitialCapital
      const actualLeverage = collateralAmount / INITIAL_COLLATERAL;

      expect(actualLeverage).to.be.closeTo(LEVERAGE_30X, 0.5); // Within 0.5x tolerance

      console.log(`  Actual leverage: ${actualLeverage.toFixed(2)}x`);
    });

    it("maintains healthy position under 97% LTV limit", async () => {
      const { program, userPositionPda } = setup;

      const position = await program.account.userPosition.fetch(userPositionPda);
      const poolAccount = await program.account.pool.fetch(setup.pool);

      // Calculate current LTV: Debt / Collateral Value
      // Assuming 1:1 collateral to lend price for this test
      const debtShares = BigInt(position.debtShares.toString());
      const totalBorrowed = BigInt(poolAccount.totalBorrowed.toString());
      const totalDebtShares = BigInt(poolAccount.totalDebtShares.toString());

      const debtAmount = Number((debtShares * totalBorrowed) / totalDebtShares);
      const collateralAmount = Number(position.collateralDeposited);

      const currentLTV = (debtAmount / collateralAmount) * 100;

      // Current LTV should be less than 97%
      expect(currentLTV).to.be.lessThan(HIGH_LTV);

      console.log(`  Current LTV: ${currentLTV.toFixed(2)}% (limit: ${HIGH_LTV}%)`);
    });
  });

  describe("leverage limits", () => {
    it("lower LTV pools support less leverage", async () => {
      // At 75% LTV (default), max leverage = 1 / (1 - 0.75) = 4x
      // setupTest already mints 1000 tokens to authority
      const setup75 = await setupTest(undefined, 75);
      await participateInPool(setup75, 500_000_000);

      const { authority, program } = setup75;
      const initialCollateral = new BN(1_000_000); // 1 token

      // 3x leverage should work at 75% LTV
      await openMultiply(setup75, authority, initialCollateral, 3);

      const [userPositionPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user_position"), setup75.pool.toBuffer(), authority.publicKey.toBuffer()],
        program.programId
      );
      const position = await program.account.userPosition.fetch(userPositionPda);
      expect(Number(position.collateralDeposited)).to.be.greaterThan(0);

      console.log("3x leverage opened successfully at 75% LTV");
    });
  });
});

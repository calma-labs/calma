import * as anchor from "@anchor-lang/core";
import { AnchorProvider, BN } from "@anchor-lang/core";
import { PublicKey, Keypair, LAMPORTS_PER_SOL, SystemProgram, Transaction } from "@solana/web3.js";
import {
  createMint,
  createAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
  createMintToInstruction,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Jbl } from "../../target/types/jbl";
import { setupTest, DEFAULT_FEE_CURVE, POOL_SPACE } from "./utils";
import { expect } from "chai";

// Hardcoded minter keypair for proof-of-concept faucet
// Copy of: app/src/hooks/useFaucet.ts
const MINTER_SECRET_KEY = new Uint8Array([
  164, 83, 220, 177, 59, 188, 88, 49, 200, 58, 85, 66, 67, 49, 29, 78,
  136, 239, 249, 139, 109, 48, 103, 122, 207, 63, 58, 166, 208, 94, 29, 195,
  235, 76, 64, 246, 35, 186, 222, 243, 110, 94, 56, 145, 95, 144, 26, 200,
  237, 159, 61, 219, 114, 138, 224, 39, 254, 99, 89, 216, 19, 83, 205, 82
]);
const HARD_CODED_MINTER = Keypair.fromSecretKey(MINTER_SECRET_KEY);

const FAUCET_AMOUNT = 1_000_000_000; // 1,000 tokens at 6 decimals

describe("hardcoded minter faucet", () => {
  let provider: AnchorProvider;
  let connection: anchor.web3.Connection;
  let payer: Keypair;
  let testMint: PublicKey;
  let user: Keypair;

  before(async () => {
    provider = AnchorProvider.env();
    anchor.setProvider(provider);
    connection = provider.connection;

    // Setup payer
    payer = Keypair.generate();
    const airdrop = await connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(airdrop);

    // Create a test user who wants tokens
    user = Keypair.generate();
    const userAirdrop = await connection.requestAirdrop(user.publicKey, 0.1 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(userAirdrop);

    // Airdrop SOL to the hardcoded minter (not strictly needed since user pays fees,
    // but ensures the account exists for signing)
    const minterAirdrop = await connection.requestAirdrop(HARD_CODED_MINTER.publicKey, 0.01 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(minterAirdrop);

    // Create a mint with the hardcoded minter as authority
    testMint = await createMint(
      connection,
      payer,
      HARD_CODED_MINTER.publicKey, // Hardcoded minter is the mint authority
      null,
      6
    );

    console.log("Mint created:", testMint.toBase58());
    console.log("Mint authority (hardcoded):", HARD_CODED_MINTER.publicKey.toBase58());
  });

  it("mints tokens to a user via the hardcoded minter (faucet style)", async () => {
    // Calculate user's ATA
    const userAta = getAssociatedTokenAddressSync(
      testMint,
      user.publicKey,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    // Build the faucet transaction
    // This mimics what useFaucet.ts does in the frontend
    const tx = new Transaction();
    tx.add(
      // Create ATA if it doesn't exist
      createAssociatedTokenAccountIdempotentInstruction(
        user.publicKey, // payer
        userAta,
        user.publicKey, // owner
        testMint,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      ),
      // Mint tokens to the user's ATA
      // The authority is the hardcoded minter, not the user
      createMintToInstruction(
        testMint,
        userAta,
        HARD_CODED_MINTER.publicKey, // mint authority
        FAUCET_AMOUNT
      )
    );

    // Set fee payer to the user (they pay for the transaction)
    tx.feePayer = user.publicKey;

    // Get recent blockhash
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;

    // Both signers use partialSign (order matters - fee payer usually last)
    // The hardcoded minter provides the mint authority signature
    tx.partialSign(HARD_CODED_MINTER);
    // The user provides the fee payer signature
    tx.partialSign(user);

    // Send the transaction
    // Get balances before transaction
    const minterBalanceBefore = await connection.getBalance(HARD_CODED_MINTER.publicKey);
    const userBalanceBefore = await connection.getBalance(user.publicKey);

    const signature = await connection.sendRawTransaction(tx.serialize());
    await connection.confirmTransaction(signature);

    console.log("Faucet transaction:", signature);

    // Verify the user received the tokens
    const { getAccount, getMint } = await import("@solana/spl-token");
    const userTokenAccount = await getAccount(connection, userAta);
    
    expect(userTokenAccount.amount.toString()).to.equal(FAUCET_AMOUNT.toString());
    console.log("User received:", Number(userTokenAccount.amount) / 1_000_000, "tokens");

    // Verify fee payment: minter's balance should be unchanged, user's should decrease
    const minterBalanceAfter = await connection.getBalance(HARD_CODED_MINTER.publicKey);
    const userBalanceAfter = await connection.getBalance(user.publicKey);

    expect(minterBalanceAfter).to.equal(minterBalanceBefore, "Minter should not pay any fees");
    expect(userBalanceAfter).to.be.lessThan(userBalanceBefore, "User should pay transaction fees");
    console.log("Minter balance:", minterBalanceBefore, "->", minterBalanceAfter, "(unchanged ✓)");
    console.log("User balance:", userBalanceBefore, "->", userBalanceAfter, "(decreased by fees ✓)");
  });

  it("allows any user to faucet without being the mint authority", async () => {
    // Create a completely new random user
    const randomUser = Keypair.generate();
    const airdrop = await connection.requestAirdrop(randomUser.publicKey, 0.1 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(airdrop);

    const userAta = getAssociatedTokenAddressSync(
      testMint,
      randomUser.publicKey,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    // Same faucet pattern - no authority check on the user
    const tx = new Transaction();
    tx.add(
      createAssociatedTokenAccountIdempotentInstruction(
        randomUser.publicKey,
        userAta,
        randomUser.publicKey,
        testMint,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      ),
      createMintToInstruction(
        testMint,
        userAta,
        HARD_CODED_MINTER.publicKey,
        FAUCET_AMOUNT
      )
    );

    tx.feePayer = randomUser.publicKey;
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;

    // Both signers use partialSign
    tx.partialSign(HARD_CODED_MINTER);
    tx.partialSign(randomUser);

    const signature = await connection.sendRawTransaction(tx.serialize());
    await connection.confirmTransaction(signature);

    const { getAccount } = await import("@solana/spl-token");
    const userTokenAccount = await getAccount(connection, userAta);
    
    expect(userTokenAccount.amount.toString()).to.equal(FAUCET_AMOUNT.toString());
    console.log("Random user received:", Number(userTokenAccount.amount) / 1_000_000, "tokens");
  });

  describe("faucet integration with pool", () => {
    it("can faucet collateral tokens and then deposit them into a pool", async () => {
      // Set up a pool using the faucet mint as collateral
      const provider = AnchorProvider.env();
      anchor.setProvider(provider);
      const program = anchor.workspace.Jbl as anchor.Program<Jbl>;

      const authority = Keypair.generate();
      const poolKeypair = Keypair.generate();
      const pool = poolKeypair.publicKey;

      // Airdrop
      const airdrop = await connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
      await connection.confirmTransaction(airdrop);

      // Create a lend mint (normal way - authority is mint auth)
      const lendMint = await createMint(connection, payer, authority.publicKey, null, 6);

      const [statePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("state")],
        program.programId
      );

      const poolRent = await connection.getMinimumBalanceForRentExemption(POOL_SPACE);
      const createPoolIx = SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: pool,
        lamports: poolRent,
        space: POOL_SPACE,
        programId: program.programId,
      });

      // Create pool with faucet mint as collateral
      await program.methods
        .create(new BN(0), new BN(50), new BN(0), new BN(0), 75)
        .accounts({
          pool,
          collateralMint: testMint, // Using the faucet-controlled mint
          lendMint,
          authority: authority.publicKey,
          payer: payer.publicKey,
        })
        .preInstructions([createPoolIx])
        .signers([payer, authority, poolKeypair])
        .rpc();

      console.log("Pool created with faucet mint as collateral");

      // Now a user faucets collateral and deposits
      const borrower = Keypair.generate();
      const borrowerAirdrop = await connection.requestAirdrop(borrower.publicKey, 0.1 * LAMPORTS_PER_SOL);
      await connection.confirmTransaction(borrowerAirdrop);

      // Faucet collateral tokens to borrower
      const borrowerCollateralAta = getAssociatedTokenAddressSync(
        testMint,
        borrower.publicKey,
        false,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      );

      const faucetTx = new Transaction();
      faucetTx.add(
        createAssociatedTokenAccountIdempotentInstruction(
          borrower.publicKey,
          borrowerCollateralAta,
          borrower.publicKey,
          testMint,
          TOKEN_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID
        ),
        createMintToInstruction(
          testMint,
          borrowerCollateralAta,
          HARD_CODED_MINTER.publicKey,
          FAUCET_AMOUNT
        )
      );

      faucetTx.feePayer = borrower.publicKey;
      const { blockhash } = await connection.getLatestBlockhash();
      faucetTx.recentBlockhash = blockhash;
      faucetTx.partialSign(HARD_CODED_MINTER);
      faucetTx.partialSign(borrower);

      await connection.sendRawTransaction(faucetTx.serialize());

      // Verify borrower has tokens
      const { getAccount } = await import("@solana/spl-token");
      const collateralAccount = await getAccount(connection, borrowerCollateralAta);
      expect(collateralAccount.amount.toString()).to.equal(FAUCET_AMOUNT.toString());

      console.log("Borrower fauceted collateral:", Number(collateralAccount.amount) / 1_000_000, "tokens");

      // Create user position for borrower
      const [userPositionPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user_position"), pool.toBuffer(), borrower.publicKey.toBuffer()],
        program.programId
      );

      // Deposit collateral into the pool
      await program.methods
        .depositCollateral(new BN(500_000_000)) // Deposit 500 tokens
        .accounts({
          pool,
          collateralMint: testMint,
          authority: borrower.publicKey,
          userTokenAccount: borrowerCollateralAta,
        })
        .signers([borrower])
        .rpc();

      console.log("Borrower deposited collateral into pool");

      // Verify collateral was deposited
      const finalCollateralAccount = await getAccount(connection, borrowerCollateralAta);
      expect(finalCollateralAccount.amount.toString()).to.equal("500000000"); // 500 tokens left
    });
  });
});

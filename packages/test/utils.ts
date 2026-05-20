import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { PublicKey, Keypair, LAMPORTS_PER_SOL, Connection, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  createAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";
import { Jbl } from "../../target/types/jbl";

/** Size of the Pool account on-chain: 8-byte discriminant + Pool struct (41 184 bytes). */
export const POOL_SPACE = 8 + 41184;

export interface FeeCurve {
  m1: BN;
  c1: BN;  // Can be negative (i64 on-chain)
  m2: BN;
  c2: BN;  // Can be negative (i64 on-chain)
}

export const DEFAULT_FEE_CURVE: FeeCurve = {
  m1: new BN(0),
  c1: new BN(50),
  m2: new BN(0),
  c2: new BN(0),
};

export interface TestSetup {
  provider: AnchorProvider;
  program: Program<Jbl>;
  connection: Connection;
  authority: Keypair;
  payer: Keypair;
  /** Mint for collateral tokens (deposited by borrowers). */
  collateralMint: PublicKey;
  /** Mint for lend tokens (deposited by lenders via participate; borrowed by borrowers). */
  lendMint: PublicKey;
  pool: PublicKey;
  statePda: PublicKey;
  collateralVaultPda: PublicKey;
  lendVaultPda: PublicKey;
  lpMintPda: PublicKey;
  userPositionPda: PublicKey;
  /** Authority's collateral token account. */
  userCollateralTokenAccount: PublicKey;
  /** Authority's lend token account. */
  userLendTokenAccount: PublicKey;
  feeCurve: FeeCurve;
}

/**
 * Sets up a complete test environment for the jbl program.
 *
 * Creates a unified pool with separate collateral and lend mints.
 * `authority` receives 1000 tokens of each mint.
 *
 * @param feeCurve - The fee curve parameters (m1, c1, m2, c2) to use when creating the pool.
 * @param ltvPercent - The LTV percentage for the pool (default: 75).
 * @returns A TestSetup object with all necessary accounts, PDAs, and program references.
 */
export async function setupTest(feeCurve: FeeCurve = DEFAULT_FEE_CURVE, ltvPercent: number = 75): Promise<TestSetup> {
  const provider = AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Jbl as Program<Jbl>;
  const connection = provider.connection;

  const authority = Keypair.generate();
  const payer = Keypair.generate();

  // Airdrop SOL to payer and authority
  const airdropPayer = await connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(airdropPayer);

  const airdropAuthority = await connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(airdropAuthority);

  // Create two test token mints (6 decimals each).
  // collateralMint: deposited by borrowers as collateral.
  // lendMint: deposited by lenders via participate; received by borrowers on borrow.
  const collateralMint = await createMint(connection, payer, authority.publicKey, null, 6);
  const lendMint = await createMint(connection, payer, authority.publicKey, null, 6);

  // Pool is a keypair account (too large for on-chain PDA allocation via CPI).
  const poolKeypair = Keypair.generate();
  const pool = poolKeypair.publicKey;

  const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state")], program.programId);

  const [collateralVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), pool.toBuffer()],
    program.programId
  );

  const [lendVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("lend_vault"), pool.toBuffer()],
    program.programId
  );

  const [lpMintPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_mint"), pool.toBuffer()],
    program.programId
  );

  const [userPositionPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("user_position"), pool.toBuffer(), authority.publicKey.toBuffer()],
    program.programId
  );

  // Create authority's collateral and lend token accounts, mint 1000 tokens each.
  const userCollateralTokenAccount = await createAssociatedTokenAccount(
    connection, payer, collateralMint, authority.publicKey
  );
  await mintTo(connection, payer, collateralMint, userCollateralTokenAccount, authority, 1_000_000_000);

  const userLendTokenAccount = await createAssociatedTokenAccount(
    connection, payer, lendMint, authority.publicKey
  );
  await mintTo(connection, payer, lendMint, userLendTokenAccount, authority, 1_000_000_000);

  // Pre-create the pool account via SystemProgram.createAccount (top-level instruction,
  // no CPI size limit).  Pool keypair signs this instruction.
  const poolRent = await connection.getMinimumBalanceForRentExemption(POOL_SPACE);
  const createPoolIx = SystemProgram.createAccount({
    fromPubkey: payer.publicKey,
    newAccountPubkey: pool,
    lamports: poolRent,
    space: POOL_SPACE,
    programId: program.programId,
  });

  // Create the lending pool.  Anchor auto-resolves collateralVault, lendVault, lpMint, state.
  await program.methods
    .create(feeCurve.m1, feeCurve.c1, feeCurve.m2, feeCurve.c2, ltvPercent)
    .accounts({
      pool,
      collateralMint,
      lendMint,
      authority: authority.publicKey,
      payer: payer.publicKey,
    })
    .preInstructions([createPoolIx])
    .signers([payer, authority, poolKeypair])
    .rpc();

  return {
    provider,
    program,
    connection,
    authority,
    payer,
    collateralMint,
    lendMint,
    pool,
    statePda,
    collateralVaultPda,
    lendVaultPda,
    lpMintPda,
    userPositionPda,
    userCollateralTokenAccount,
    userLendTokenAccount,
    feeCurve,
  };
}

export interface Lender {
  authority: Keypair;
  /** Collateral token account — used as source for deposit and destination for withdraw. */
  userTokenAccount: PublicKey;
  /** Lend token account — used as source for participate and destination for borrow. */
  userLendTokenAccount: PublicKey;
  userPositionPda: PublicKey;
}

/**
 * Creates a new user (borrower or lender) for an existing pool.
 * Airdrops SOL, creates token accounts for both mints, and mints 1000 tokens each.
 */
export async function createLender(setup: TestSetup): Promise<Lender> {
  const { connection, payer, collateralMint, lendMint, authority: mintAuthority, program, pool } = setup;

  const authority = Keypair.generate();

  const airdrop = await connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(airdrop);

  const userTokenAccount = await createAssociatedTokenAccount(connection, payer, collateralMint, authority.publicKey);
  await mintTo(connection, payer, collateralMint, userTokenAccount, mintAuthority, 1_000_000_000);

  const userLendTokenAccount = await createAssociatedTokenAccount(connection, payer, lendMint, authority.publicKey);
  await mintTo(connection, payer, lendMint, userLendTokenAccount, mintAuthority, 1_000_000_000);

  const [userPositionPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("user_position"), pool.toBuffer(), authority.publicKey.toBuffer()],
    program.programId
  );

  return { authority, userTokenAccount, userLendTokenAccount, userPositionPda };
}

/**
 * Deposits lend tokens from setup.authority into the pool's lend vault via `participate`.
 * Provides liquidity so borrowers can borrow.
 */
export async function participateInPool(setup: TestSetup, amount: number): Promise<void> {
  await setup.program.methods
    .depositLent(new BN(amount))
    .accounts({
      pool: setup.pool,
      lendMint: setup.lendMint,
      authority: setup.authority.publicKey,
      userLendTokenAccount: setup.userLendTokenAccount,
    })
    .signers([setup.authority])
    .rpc();
}

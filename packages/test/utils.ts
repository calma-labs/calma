import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { PublicKey, Keypair, LAMPORTS_PER_SOL, Connection, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  setAuthority,
  AuthorityType,
} from "@solana/spl-token";
import { Calma } from "../../target/types/calma";
import { Irm } from "../../target/types/irm";
import { Feed } from "../../target/types/feed";
import { Guard } from "../../target/types/guard";
import { Faucet } from "../../target/types/faucet";

import {
  collateral_vault_seed,
  feed_seed,
  irm_config_seed,
  lend_vault_seed,
  lp_mint_seed,
  state_seed,
  user_position_seed,
} from "@calma/wasm-lib";

import CalmaIdl from "../../target/idl/calma.json";
import GuardIdl from "../../target/idl/guard.json";
import FaucetIdl from "../../target/idl/faucet.json";

export const POOL_SPACE: number = Number(
  CalmaIdl.constants.find((c: { name: string }) => c.name === "POOL_SPACE")!.value
);

/** Read a `#[constant]` string out of a generated IDL.
 *
 * `guard` and `faucet` have no `*-state` crate for the wasm bindings to export
 * from, so their seeds travel through the IDL instead. Values are quoted there. */
export function idlStringConstant(
  idl: { constants: { name: string; value: string }[] },
  name: string,
): string {
  const raw = idl.constants.find((c) => c.name === name)?.value;
  if (raw === undefined) throw new Error(`${name} missing from IDL — run \`anchor build\``);
  return JSON.parse(raw) as string;
}

export const GUARD_SEED = idlStringConstant(GuardIdl, "GUARD_SEED");
export const MINT_AUTHORITY_SEED = idlStringConstant(FaucetIdl, "MINT_AUTHORITY_SEED");

export interface TestSetup {
  provider: AnchorProvider;
  program: Program<Calma>;
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
  irmConfig: PublicKey;
  irmProgramId: PublicKey;
  feedProgram: Program<Feed>;
  feedPda: PublicKey;
  feedAuthority: PublicKey;
  /** Test-only 1:1 burn/mint faucet, split out of the calma program. */
  faucetProgram: Program<Faucet>;
  /** The faucet's `[b"mint_authority"]` PDA — mint authority for every faucet-owned mint. */
  faucetMintAuthorityPda: PublicKey;
}

/**
 * Sets up a complete test environment for the calma program.
 *
 * Creates a unified pool with separate collateral and lend mints.
 * `authority` receives 1000 tokens of each mint.
 *
 * @param feeCurve - The fee curve parameters (m1, c1, m2, c2) to use when creating the pool.
 * @param ltvPercent - The LTV percentage for the pool (default: 75).
 * @returns A TestSetup object with all necessary accounts, PDAs, and program references.
 */
/**
 * One whitelist shared by the whole test run. Whitelists are per-authority
 * (`["guard", authority]`), so several can coexist — tests that need a *second*
 * subset should call `createGuard` for a fresh authority rather than reusing
 * this one.
 */
let sharedGuard: { pda: PublicKey; authority: Keypair } | null = null;

export function findGuardPda(guardProgramId: PublicKey, authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(GUARD_SEED), authority.toBuffer()],
    guardProgramId
  )[0];
}

/** Stand up a fresh whitelist under a newly generated authority. */
export async function createGuard(
  connection: Connection,
  guardProgram: Program<Guard>,
  payer: Keypair
): Promise<{ pda: PublicKey; authority: Keypair }> {
  const authority = Keypair.generate();
  await connection.confirmTransaction(
    await connection.requestAirdrop(authority.publicKey, LAMPORTS_PER_SOL)
  );
  const pda = findGuardPda(guardProgram.programId, authority.publicKey);
  if (!(await connection.getAccountInfo(pda))) {
    await guardProgram.methods
      .create()
      .accounts({ authority: authority.publicKey, payer: payer.publicKey })
      .signers([payer, authority])
      .rpc();
  }
  return { pda, authority };
}

/**
 * The shared whitelist, created on first use. Every file must go through this
 * rather than rolling its own — two independent lazy initialisers would each
 * create a *different* per-authority list, and a pool pinned to one would reject
 * members added to the other.
 */
export async function ensureGuard(
  connection: Connection,
  guardProgram: Program<Guard>,
  payer: Keypair
): Promise<{ pda: PublicKey; authority: Keypair }> {
  if (sharedGuard === null) {
    sharedGuard = await createGuard(connection, guardProgram, payer);
  }
  return sharedGuard;
}

export async function whitelistAuthority(
  connection: Connection,
  guardProgram: Program<Guard>,
  payer: Keypair,
  authority: PublicKey
): Promise<{ program: PublicKey; state: PublicKey }> {
  const guard = await ensureGuard(connection, guardProgram, payer);
  await guardProgram.methods
    .add(authority)
    .accounts({ guardState: guard.pda, authority: guard.authority.publicKey })
    .signers([guard.authority])
    .rpc();
  return { program: guardProgram.programId, state: guard.pda };
}

export async function setupTest(
  ltvPercent: number = 75,
  opts: {
    poolKeypair?: Keypair;
    rateProgram?: PublicKey;
    rateState?: PublicKey;
    /**
     * Pool authority. Supply this whenever you pre-create the IRM yourself
     * (i.e. alongside `rateProgram`/`rateState`) — `calma::create` requires the
     * market authority to own its rate curve, so both must be the same key.
     */
    authority?: Keypair;
  } = {}
): Promise<TestSetup> {
  const provider = AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Calma as Program<Calma>;
  const connection = provider.connection;

  const authority = opts.authority ?? Keypair.generate();
  const payer = Keypair.generate();

  // Airdrop SOL to payer and authority
  const airdropPayer = await connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(airdropPayer);

  // A caller-supplied authority is usually already funded; a second identical
  // airdrop can be rejected as a duplicate transaction.
  if ((await connection.getBalance(authority.publicKey)) === 0) {
    const airdropAuthority = await connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(airdropAuthority);
  }

  // Create two test token mints (6 decimals each).
  // collateralMint: deposited by borrowers as collateral.
  // lendMint: deposited by lenders via participate; received by borrowers on borrow.
  const collateralMint = await createMint(connection, payer, authority.publicKey, null, 6);
  const lendMint = await createMint(connection, payer, authority.publicKey, null, 6);

  // Pool is a keypair account (too large for on-chain PDA allocation via CPI).
  const poolKeypair = opts.poolKeypair ?? Keypair.generate();
  const pool = poolKeypair.publicKey;

  const feedProgram = anchor.workspace.Feed as Program<Feed>;
  const faucetProgram = anchor.workspace.Faucet as Program<Faucet>;
  const guardProgram = anchor.workspace.Guard as Program<Guard>;
  const feedAuthority = provider.wallet.publicKey;
  const [feedPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(feed_seed()), collateralMint.toBuffer(), lendMint.toBuffer(), Buffer.from([0])],
    feedProgram.programId
  );

  const irmProgram = anchor.workspace.Irm as Program<Irm>;
  const [irmConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(irm_config_seed()), pool.toBuffer()],
    irmProgram.programId
  );
  const irmProgramId = opts.rateProgram ?? irmProgram.programId;
  const irmConfig = opts.rateState ?? irmConfigPda;

  const [faucetMintAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(MINT_AUTHORITY_SEED)],
    faucetProgram.programId
  );

  const [statePda] = PublicKey.findProgramAddressSync([Buffer.from(state_seed())], program.programId);

  const [collateralVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(collateral_vault_seed()), pool.toBuffer()],
    program.programId
  );

  const [lendVaultPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(lend_vault_seed()), pool.toBuffer()],
    program.programId
  );

  const [lpMintPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(lp_mint_seed()), pool.toBuffer()],
    program.programId
  );

  const [userPositionPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(user_position_seed()), pool.toBuffer(), authority.publicKey.toBuffer()],
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

  // Initialize the IRM for this pool (unless the caller supplied an explicit rate program).
  if (!opts.rateProgram) {
    await irmProgram.methods
      .initialize([
        { utilBps: 0, rateBps: 0 },
        { utilBps: 10_000, rateBps: 500 },
      ])
      .accounts({ pool, authority: authority.publicKey, payer: payer.publicKey })
      .signers([payer, authority])
      .rpc();
  }

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

  // Create the feed account once; skip if already exists (shared provider wallet key).
  // Manual price source: feed ids must be all-zero and age rule is inert.
  if (!(await connection.getAccountInfo(feedPda))) {
    await feedProgram.methods
      .create(
        0,
        { manual: {} },
        Array(32).fill(0),
        Array(32).fill(0),
        90_000,
        { maxConfBps: 0, maxDeviationBpsPerHour: 0, emaDivergenceBps: 0, minPrice: new BN(0), maxPrice: new BN(0), maxAgeMs: 0, reserved: Array(4).fill(0) }
      )
      .accounts({
        feed: feedPda,
        authority: feedAuthority,
        collateralMint,
        lendMint,
        payer: payer.publicKey,
      })
      .signers([payer])
      .rpc();
  }

  // Set initial oracle price (collateral == lend == 1.0) so the ratio is 1.0.
  // Must land before `create`, which reads the feed and refuses a zero price.
  await feedProgram.methods
    .setValue(new BN(1_000_000), new BN(1_000_000))
    .accounts({ authority: feedAuthority, feed: feedPda })
    .rpc();

  // Create the lending pool.  Anchor auto-resolves collateralVault, lendVault, lpMint, state.
  await program.methods
    .create(ltvPercent)
    .accounts({
      pool,
      collateralMint,
      lendMint,
      authority: authority.publicKey,
      payer: payer.publicKey,
      feedState: feedPda,
      rateProgram: irmProgramId,
      irmState: irmConfig,
        guardProgram: null,
        guardState: null,
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
    irmConfig,
    irmProgramId,
    feedProgram,
    feedPda,
    feedAuthority,
    faucetProgram,
    faucetMintAuthorityPda,
  };
}

/**
 * Hands both of the pool's mints over to the faucet's mint-authority PDA.
 *
 * `mock_swap` mints via that PDA, so it only accepts faucet-owned mints. Call this
 * after `setupTest()` has finished funding users — once authority moves, the
 * `authority` keypair can no longer `mintTo` (use `faucetProgram.methods.mint`).
 */
export async function transferMintsToFaucet(setup: TestSetup): Promise<void> {
  const { connection, payer, authority, collateralMint, lendMint, faucetMintAuthorityPda } = setup;

  for (const mint of [collateralMint, lendMint]) {
    await setAuthority(
      connection,
      payer,
      mint,
      authority,
      AuthorityType.MintTokens,
      faucetMintAuthorityPda
    );
  }
}

/**
 * Updates the shared manual oracle feed's collateral and lend prices.
 *
 * With equal token decimals (all test mints use 6), the borrow check reads
 * `ratio = collateralPrice / lendPrice × PRICE_SCALE`, so this directly scales a
 * position's borrow capacity: capacity = collateral × ratio / PRICE_SCALE × LTV.
 */
export async function setFeedPrice(
  setup: TestSetup,
  collateralPrice: number,
  lendPrice: number = 1_000_000
): Promise<void> {
  await setup.feedProgram.methods
    .setValue(new BN(collateralPrice), new BN(lendPrice))
    .accounts({ authority: setup.feedAuthority, feed: setup.feedPda })
    .rpc();
}

/**
 * Retunes how long the feed's written price stays consumable.
 *
 * The budget lives on the feed, not the pool, so this changes the `StaleOracle`
 * gate for every market pricing against it. `0` is rejected on-chain — it is the
 * fail-closed sentinel, not a way to disable the check.
 */
export async function setFeedTtl(setup: TestSetup, priceTtlMs: number): Promise<void> {
  await setup.feedProgram.methods
    .setPriceTtl(priceTtlMs)
    .accounts({ authority: setup.feedAuthority, feed: setup.feedPda })
    .rpc();
}

export function irmAccounts(setup: TestSetup) {
  return [
    { pubkey: setup.irmProgramId, isWritable: false, isSigner: false },
    { pubkey: setup.irmConfig, isWritable: false, isSigner: false },
  ];
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
    [Buffer.from(user_position_seed()), pool.toBuffer(), authority.publicKey.toBuffer()],
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
      guardProgram: null,
      guardState: null,
      pool: setup.pool,
      lendMint: setup.lendMint,
      authority: setup.authority.publicKey,
      userLendTokenAccount: setup.userLendTokenAccount,
      rateProgram: setup.irmProgramId,
      irmState: setup.irmConfig,
    })
    .signers([setup.authority])
    .rpc();
}

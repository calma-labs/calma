import { Surfnet } from "@solana/surfpool";
import * as anchor from "@anchor-lang/core";
import { AnchorProvider, BN, Wallet } from "@anchor-lang/core";
import {
  Connection,
  Keypair,
  PublicKey,
  LAMPORTS_PER_SOL,
  SystemProgram,
} from "@solana/web3.js";
import {
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";

// The push oracle program creates and updates the sponsored PriceUpdateV2 accounts.
// PDA derivation uses this program ID; account ownership is the receiver program below.
const PYTH_PUSH_ORACLE = new PublicKey("pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT");
// The receiver program is the on-chain owner of PriceUpdateV2 accounts.
const PYTH_RECEIVER    = new PublicKey("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

// Well-known Pyth price feed ids (hex, no 0x). Same across every Solana cluster.
const SOL_USD  = "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";
const USDC_USD = "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a";

/**
 * Shard-0 PDA of [u16-LE-zero, feed_id_32b] under the push oracle program —
 * the deterministic address of the sponsored PriceUpdateV2 account for a feed.
 * Mirrors `getPriceFeedAccountForProgram(0, feedId)` from @pythnetwork/pyth-solana-receiver.
 */
function sponsoredPushAccount(feedIdHex: string): PublicKey {
  const shard = Buffer.alloc(2); // shard 0, u16 LE
  const [pda] = PublicKey.findProgramAddressSync(
    [shard, Buffer.from(feedIdHex, "hex")],
    PYTH_PUSH_ORACLE,
  );
  return pda;
}

const NO_RULES = {
  maxConfBps: 0,
  maxDeviationBpsPerHour: 0,
  emaDivergenceBps: 0,
  minPrice: new BN(0),
  maxPrice: new BN(0),
  maxAgeMs: 0,
  reserved: Array(4).fill(0),
};

// Pre-derive the two sponsored accounts so they're visible at describe time.
const collateralPush = sponsoredPushAccount(SOL_USD);
const lendPush       = sponsoredPushAccount(USDC_USD);

describe("surfpool borrow against mainnet Pyth (sponsored push)", () => {
  const LEND_LIQUIDITY     = 500_000_000;
  const COLLATERAL_DEPOSIT = 1_000_000_000;
  const BORROW_AMOUNT      = 1_000_000; // 1 token — safely under any LTV

  let surfnet: Surfnet;
  let provider: AnchorProvider;
  let calma: any;
  let feed: any;
  let irm: any;
  let payer: Keypair;
  let authority: Keypair;
  let collateralMint: PublicKey;
  let lendMint: PublicKey;
  let pool: Keypair;
  let feedPda: PublicKey;
  let irmConfigPda: PublicKey;
  let lendVaultPda: PublicKey;
  let userPositionPda: PublicKey;
  let userCollateralAta: PublicKey;
  let userLendAta: PublicKey;

  before(async function () {
    this.timeout(600_000);

    // Use clock-mode blocks so confirmTransaction never stalls waiting for
    // a block that never comes (default "transaction" mode sometimes hangs
    // while web3.js waits for block height to advance via WS subscription).
    surfnet = Surfnet.startWithConfig({
      offline: false,
      remoteRpcUrl: "https://api.mainnet-beta.solana.com",
      blockProductionMode: "clock",
      slotTimeMs: 400,
    });

    // Align the local clock with wall time so mainnet publish_time is fresh.
    surfnet.timeTravelToTimestamp(Date.now());

    // Register the two sponsored PriceUpdateV2 accounts for background streaming.
    surfnet.streamAccount(collateralPush.toBase58());
    surfnet.streamAccount(lendPush.toBase58());

    // Deploy the three programs from local Anchor build artifacts.
    surfnet.deployProgram("calma");
    surfnet.deployProgram("feed");
    surfnet.deployProgram("irm");

    // Build the Connection with the explicit WS endpoint surfpool exposes.
    // Without this, web3.js opens a WS to localhost:80 by default and all
    // confirmTransaction calls stall until block height expires.
    const connection = new Connection(surfnet.rpcUrl, {
      commitment: "confirmed",
      wsEndpoint: surfnet.wsUrl,
    });
    payer = Keypair.fromSecretKey(surfnet.payerSecretKey);
    provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
    anchor.setProvider(provider);
    calma  = anchor.workspace.Calma;
    feed = anchor.workspace.Feed;
    irm  = anchor.workspace.Irm;

    authority = Keypair.generate();
    surfnet.fundSol(authority.publicKey.toBase58(), 5 * LAMPORTS_PER_SOL);

    // Local test mints (6 decimals each). The feed pins to Pyth pubkeys, not
    // to these mints, so their addresses only affect the decimal adjustment in
    // the ratio (equal decimals → ratio ≈ SOL price in USDC terms).
    collateralMint = await createMint(connection, payer, authority.publicKey, null, 6);
    lendMint       = await createMint(connection, payer, authority.publicKey, null, 6);

    // Poll until surfpool has fetched the streamed Pyth accounts from mainnet.
    for (let i = 0; i < 60; i++) {
      const [c, l] = await Promise.all([
        connection.getAccountInfo(collateralPush),
        connection.getAccountInfo(lendPush),
      ]);
      if (c?.owner.equals(PYTH_RECEIVER) && l?.owner.equals(PYTH_RECEIVER)) break;
      await new Promise((r) => setTimeout(r, 500));
    }
    const collInfo = await connection.getAccountInfo(collateralPush);
    const lendInfo = await connection.getAccountInfo(lendPush);
    expect(collInfo, "SOL/USD sponsored account not streamed from mainnet").to.not.equal(null);
    expect(lendInfo, "USDC/USD sponsored account not streamed from mainnet").to.not.equal(null);
    expect(collInfo!.owner.toBase58()).to.equal(PYTH_RECEIVER.toBase58());

    // Create the PythPush feed pinned to the two sponsored account pubkeys.
    [feedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("feed"), collateralMint.toBuffer(), lendMint.toBuffer(), Buffer.from([0])],
      feed.programId,
    );
    await feed.methods
      .create(
        0,
        { pythPush: {} },
        Array.from(collateralPush.toBytes()),
        Array.from(lendPush.toBytes()),
        { ...NO_RULES, maxAgeMs: 3_600_000 }, // large max-age so any clock skew won't block
      )
      .accounts({ feed: feedPda, authority: authority.publicKey, collateralMint, lendMint, payer: payer.publicKey })
      .signers([payer, authority])
      .rpc();

    // Push mainnet prices into the feed. Must happen before pool creation
    // because the pool's create handler reads the oracle via CPI.
    await feed.methods
      .setFromPythPush()
      .accountsPartial({ feed: feedPda, collateralPriceUpdate: collateralPush, lendPriceUpdate: lendPush })
      .rpc();

    const feedInfo = await connection.getAccountInfo(feedPda);
    const feedState = feed.coder.accounts.decode("feed", feedInfo!.data);
    expect(feedState.state.collateralPrice.toNumber(), "SOL price should be > 0").to.be.greaterThan(0);
    expect(feedState.state.lendPrice.toNumber(), "USDC price should be > 0").to.be.greaterThan(0);

    // Derive pool PDAs.
    pool = Keypair.generate();
    [irmConfigPda]  = PublicKey.findProgramAddressSync([Buffer.from("irm_config"), pool.publicKey.toBuffer()], irm.programId);
    [lendVaultPda]  = PublicKey.findProgramAddressSync([Buffer.from("lend_vault"), pool.publicKey.toBuffer()], calma.programId);
    [userPositionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("user_position"), pool.publicKey.toBuffer(), authority.publicKey.toBuffer()],
      calma.programId,
    );

    // Mint tokens for authority.
    userCollateralAta = await createAssociatedTokenAccount(connection, payer, collateralMint, authority.publicKey);
    await mintTo(connection, payer, collateralMint, userCollateralAta, authority, COLLATERAL_DEPOSIT);
    userLendAta = await createAssociatedTokenAccount(connection, payer, lendMint, authority.publicKey);
    await mintTo(connection, payer, lendMint, userLendAta, authority, LEND_LIQUIDITY + BORROW_AMOUNT * 10);

    // Initialize the IRM for this pool.
    await irm.methods
      .initialize([{ utilBps: 0, rateBps: 0 }, { utilBps: 10_000, rateBps: 500 }])
      .accounts({ pool: pool.publicKey, authority: authority.publicKey, payer: payer.publicKey })
      .signers([payer, authority])
      .rpc();

    // Pre-allocate the pool account (too large for on-chain CPI).
    const POOL_SPACE  = calma.account.pool.size;
    const poolRent    = await connection.getMinimumBalanceForRentExemption(POOL_SPACE);
    const createPoolIx = SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: pool.publicKey,
      lamports: poolRent,
      space: POOL_SPACE,
      programId: calma.programId,
    });
    await calma.methods
      .create(75, 90)
      .accounts({
        pool: pool.publicKey, collateralMint, lendMint,
        authority: authority.publicKey, payer: payer.publicKey,
        feedProgram: feed.programId, feedState: feedPda,
        rateProgram: irm.programId,  irmState: irmConfigPda,
        guardProgram: null, guardState: null,
      })
      .preInstructions([createPoolIx])
      .signers([payer, authority, pool])
      .rpc();

    // Seed the pool's lend vault so there is liquidity to borrow.
    await calma.methods
      .depositLent(new BN(LEND_LIQUIDITY))
      .accounts({ pool: pool.publicKey, lendMint, authority: authority.publicKey, userLendTokenAccount: userLendAta })
      .signers([authority])
      .rpc();

    // Post collateral from the borrower.
    await calma.methods
      .depositCollateral(new BN(COLLATERAL_DEPOSIT))
      .accounts({ pool: pool.publicKey, collateralMint, authority: authority.publicKey, userTokenAccount: userCollateralAta })
      .signers([authority])
      .rpc();
  });

  after(() => {
    (provider.connection as any)._rpcWebSocket.close();
    surfnet.stop();
  });

  it("borrows lend tokens priced by a live mainnet Pyth sponsored feed", async () => {
    await calma.methods
      .borrow(new BN(BORROW_AMOUNT))
      .accounts({
        pool: pool.publicKey, lendMint,
        authority: authority.publicKey,
        rateProgram: irm.programId, irmState: irmConfigPda,
        feedProgram: feed.programId, feedState: feedPda,
      })
      .signers([authority])
      .rpc();

    const poolAcc = await calma.account.pool.fetch(pool.publicKey);
    expect(poolAcc.market.totalBorrowAssets.toString()).to.equal(BORROW_AMOUNT.toString());

    const position = await calma.account.userPosition.fetch(userPositionPda);
    expect(position.debtShares.toNumber()).to.be.greaterThan(0);

    const lendVault = await getAccount(provider.connection, lendVaultPda);
    expect(Number(lendVault.amount)).to.equal(LEND_LIQUIDITY - BORROW_AMOUNT);
  });
});

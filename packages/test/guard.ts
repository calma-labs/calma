import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { createMint } from "@solana/spl-token";
import { expect } from "chai";
import { Guard } from "../../target/types/guard";
import { Calma } from "../../target/types/calma";
import { Feed } from "../../target/types/feed";
import { Irm } from "../../target/types/irm";
import { POOL_SPACE } from "./utils";

function findGuardPda(authority: PublicKey, programId: PublicKey): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("guard"), authority.toBuffer()],
    programId
  );
  return pda;
}

describe("guard program", () => {
  const provider = AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Guard as Program<Guard>;

  let payer: Keypair;
  let authority: Keypair;
  let guardPda: PublicKey;

  before(async () => {
    payer = Keypair.generate();
    authority = Keypair.generate();

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL)
    );
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(authority.publicKey, LAMPORTS_PER_SOL)
    );

    guardPda = findGuardPda(authority.publicKey, program.programId);
  });

  it("create initialises an empty whitelist", async () => {
    await program.methods
      .create()
      .accounts({ authority: authority.publicKey, payer: payer.publicKey })
      .signers([payer, authority])
      .rpc();

    const state = await program.account.guardState.fetch(guardPda);
    expect(state.authority.toString()).to.equal(authority.publicKey.toString());
    expect(state.whitelist).to.have.length(0);
  });

  it("add puts a pubkey in the whitelist", async () => {
    const target = Keypair.generate().publicKey;

    await program.methods
      .add(target)
      .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
      .signers([authority])
      .rpc();

    const state = await program.account.guardState.fetch(guardPda);
    expect(state.whitelist.map((p: PublicKey) => p.toString())).to.include(target.toString());
  });

  it("add rejects a duplicate entry", async () => {
    const state = await program.account.guardState.fetch(guardPda);
    const existing = state.whitelist[0] as PublicKey;

    try {
      await program.methods
        .add(existing)
        .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
        .signers([authority])
        .rpc();
      expect.fail("expected duplicate add to fail");
    } catch (e: any) {
      expect(e.message).to.include("AlreadyWhitelisted");
    }
  });

  it("remove takes a pubkey out of the whitelist", async () => {
    const state = await program.account.guardState.fetch(guardPda);
    const target = state.whitelist[0] as PublicKey;

    await program.methods
      .remove(target)
      .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
      .signers([authority])
      .rpc();

    const updated = await program.account.guardState.fetch(guardPda);
    expect(updated.whitelist.map((p: PublicKey) => p.toString())).to.not.include(target.toString());
  });

  it("check passes for a whitelisted pubkey", async () => {
    const target = Keypair.generate().publicKey;
    await program.methods
      .add(target)
      .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
      .signers([authority])
      .rpc();

    // check is a read-only CPI instruction; simulate it to confirm it succeeds
    await program.methods
      .check(target)
      .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
      .simulate();
  });

  it("check fails for a non-whitelisted pubkey", async () => {
    const stranger = Keypair.generate().publicKey;

    try {
      await program.methods
        .check(stranger)
        .accounts({ guardState: guardPda, authority: authority.publicKey } as any)
        .rpc();
      expect.fail("expected check to fail for non-whitelisted pubkey");
    } catch (e: any) {
      expect(e.message).to.include("NotWhitelisted");
    }
  });
});

describe("calma create with guard", () => {
  const provider = AnchorProvider.env();
  anchor.setProvider(provider);

  const calmaProgram = anchor.workspace.Calma as Program<Calma>;
  const guardProgram = anchor.workspace.Guard as Program<Guard>;
  const feedProgram = anchor.workspace.Feed as Program<Feed>;
  const irmProgram = anchor.workspace.Irm as Program<Irm>;

  let payer: Keypair;
  let guardAuthority: Keypair;
  let guardPda: PublicKey;
  let collateralMint: PublicKey;
  let lendMint: PublicKey;
  let feedPda: PublicKey;

  before(async () => {
    payer = Keypair.generate();
    guardAuthority = Keypair.generate();

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(payer.publicKey, 4 * LAMPORTS_PER_SOL)
    );
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(guardAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    collateralMint = await createMint(provider.connection, payer, payer.publicKey, null, 6);
    lendMint = await createMint(provider.connection, payer, payer.publicKey, null, 6);

    // Create the feed once (shared across both sub-tests).
    [feedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("feed"), collateralMint.toBuffer(), lendMint.toBuffer(), Buffer.from([0])],
      feedProgram.programId
    );
    if (!(await provider.connection.getAccountInfo(feedPda))) {
      await feedProgram.methods
        .create(
          0,
          { manual: {} },
          Array(32).fill(0),
          Array(32).fill(0),
          { maxConfBps: 0, maxDeviationBpsPerHour: 0, emaDivergenceBps: 0, minPrice: new BN(0), maxPrice: new BN(0), maxAgeMs: 0, reserved: Array(4).fill(0) }
        )
        .accounts({ authority: payer.publicKey, collateralMint, lendMint, payer: payer.publicKey })
        .signers([payer])
        .rpc();
    }
    await feedProgram.methods
      .setValue(new BN(1_000_000), new BN(1_000_000))
      .accounts({ authority: payer.publicKey, feed: feedPda })
      .signers([payer])
      .rpc();

    // Deploy a guard owned by guardAuthority.
    guardPda = findGuardPda(guardAuthority.publicKey, guardProgram.programId);
    await guardProgram.methods
      .create()
      .accounts({ authority: guardAuthority.publicKey, payer: payer.publicKey })
      .signers([payer, guardAuthority])
      .rpc();
  });

  /** Shared helper: allocates pool account, initialises IRM, then calls calma::create. */
  async function createPool(poolAuthority: Keypair, guard: { program: PublicKey; state: PublicKey } | null) {
    const poolKeypair = Keypair.generate();
    const pool = poolKeypair.publicKey;

    const [irmConfig] = PublicKey.findProgramAddressSync(
      [Buffer.from("irm_config"), pool.toBuffer()],
      irmProgram.programId
    );

    await irmProgram.methods
      .initialize([
        { utilBps: 0, rateBps: 0 },
        { utilBps: 10_000, rateBps: 500 },
      ])
      .accounts({ pool, authority: poolAuthority.publicKey, payer: payer.publicKey })
      .signers([payer, poolAuthority])
      .rpc();

    const poolRent = await provider.connection.getMinimumBalanceForRentExemption(POOL_SPACE);
    const createPoolAccountIx = SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: pool,
      lamports: poolRent,
      space: POOL_SPACE,
      programId: calmaProgram.programId,
    });

    await calmaProgram.methods
      .create(75, 90)
      .accounts({
        pool,
        collateralMint,
        lendMint,
        authority: poolAuthority.publicKey,
        payer: payer.publicKey,
        feedProgram: feedProgram.programId,
        feedState: feedPda,
        rateProgram: irmProgram.programId,
        irmState: irmConfig,
        guardProgram: guard?.program ?? null,
        guardState: guard?.state ?? null,
      })
      .preInstructions([createPoolAccountIx])
      .signers([payer, poolAuthority, poolKeypair])
      .rpc();

    return pool;
  }

  it("succeeds when authority is whitelisted", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    // Add the pool authority to the guard whitelist.
    await guardProgram.methods
      .add(poolAuthority.publicKey)
      .accounts({ guardState: guardPda, authority: guardAuthority.publicKey } as any)
      .signers([guardAuthority])
      .rpc();

    const pool = await createPool(poolAuthority, {
      program: guardProgram.programId,
      state: guardPda,
    });

    // Verify pool account exists on-chain.
    const info = await provider.connection.getAccountInfo(pool);
    expect(info).to.not.be.null;
  });

  it("fails when authority is not whitelisted", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    // Do NOT add poolAuthority to the whitelist.
    try {
      await createPool(poolAuthority, {
        program: guardProgram.programId,
        state: guardPda,
      });
      expect.fail("expected pool creation to fail for non-whitelisted authority");
    } catch (e: any) {
      expect(e.message).to.include("NotWhitelisted");
    }
  });

  it("succeeds without a guard (open pool creation)", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    const pool = await createPool(poolAuthority, null);
    const info = await provider.connection.getAccountInfo(pool);
    expect(info).to.not.be.null;
  });
});

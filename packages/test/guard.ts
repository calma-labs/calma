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
import { GUARD_SEED, POOL_SPACE, createGuard, ensureGuard, whitelistAuthority } from "./utils";

function localFindGuardPda(programId: PublicKey, authority: PublicKey): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    // Per-authority whitelist, so several subsets can coexist. Safe only
    // because consumers pin the *state* account, not just the program: `calma`
    // records it in `Pool.guard_state` at market creation and rejects anything
    // else. See guard::instructions::initialize.
    [Buffer.from(GUARD_SEED), authority.toBuffer()],
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

    const guard = await ensureGuard(provider.connection, program, payer);
    guardPda = guard.pda;
    authority = guard.authority;
  });

  it("create initialises an empty whitelist", async () => {
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
          90_000,
          { maxConfBps: 0, maxDeviationBpsPerHour: 0, emaDivergenceBps: 0, minPrice: new BN(0), maxPrice: new BN(0), maxAgeMs: 0, reserved: Array(4).fill(0) }
        )
        .accounts({ feed: feedPda, authority: payer.publicKey, collateralMint, lendMint, payer: payer.publicKey })
        .signers([payer])
        .rpc();
    }
    await feedProgram.methods
      .setValue(new BN(1_000_000), new BN(1_000_000))
      .accounts({ authority: payer.publicKey, feed: feedPda })
      .signers([payer])
      .rpc();

    // Adopt the run-shared whitelist (creating it if this block runs first).
    const guard = await ensureGuard(provider.connection, guardProgram, payer);
    guardPda = guard.pda;
    guardAuthority = guard.authority;
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
      .create(75)
      .accounts({
        pool,
        collateralMint,
        lendMint,
        authority: poolAuthority.publicKey,
        payer: payer.publicKey,
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

  it("succeeds without a guard — creation is permissionless", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    const pool = await createPool(poolAuthority, null);
    const info = await provider.connection.getAccountInfo(pool);
    expect(info).to.not.be.null;
  });

  it("records the chosen whitelist on the pool", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );
    await guardProgram.methods
      .add(poolAuthority.publicKey)
      .accounts({ guardState: guardPda, authority: guardAuthority.publicKey } as any)
      .signers([guardAuthority])
      .rpc();

    // The PDA is derived from the guard's own authority, so it is reproducible
    // off-chain from that pubkey alone.
    expect(localFindGuardPda(guardProgram.programId, guardAuthority.publicKey).toString()).to.equal(
      guardPda.toString()
    );

    const gated = await createPool(poolAuthority, {
      program: guardProgram.programId,
      state: guardPda,
    });
    const gatedPool = await calmaProgram.account.pool.fetch(gated);
    expect(gatedPool.guardState.toString()).to.equal(guardPda.toString());

    // An ungated market records the default pubkey and is open to everyone.
    const openAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(openAuthority.publicKey, LAMPORTS_PER_SOL)
    );
    const open = await createPool(openAuthority, null);
    const openPool = await calmaProgram.account.pool.fetch(open);
    expect(openPool.guardState.toString()).to.equal(PublicKey.default.toString());
  });

  it("whitelists are per-authority, so several can coexist", async () => {
    const second = await createGuard(provider.connection, guardProgram, payer);
    expect(second.pda.toString()).to.not.equal(guardPda.toString());
    expect(second.pda.toString()).to.equal(
      localFindGuardPda(guardProgram.programId, second.authority.publicKey).toString()
    );

    // Membership does not leak between lists.
    const member = Keypair.generate().publicKey;
    await guardProgram.methods
      .add(member)
      .accounts({ guardState: second.pda, authority: second.authority.publicKey } as any)
      .signers([second.authority])
      .rpc();

    const a = await guardProgram.account.guardState.fetch(guardPda);
    const b = await guardProgram.account.guardState.fetch(second.pda);
    expect(b.whitelist.map((k: PublicKey) => k.toString())).to.include(member.toString());
    expect(a.whitelist.map((k: PublicKey) => k.toString())).to.not.include(member.toString());
  });

  it("rejects a guard_state that is not a guard account", async () => {
    // Creation lets the creator *choose* a whitelist, but the CPI still has to
    // land on a real, canonically derived one — the guard program re-derives
    // ["guard", authority] from the account's own recorded authority.
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    try {
      await createPool(poolAuthority, {
        program: guardProgram.programId,
        state: Keypair.generate().publicKey,
      });
      expect.fail("expected pool creation with a non-guard account to fail");
    } catch (e: any) {
      expect(e.message).to.match(/AccountNotInitialized|AccountOwnedByWrongProgram|ConstraintSeeds/);
    }
  });

  // `create` used to require `guard_program == guard::ID`. It cannot any more:
  // `calma` links no guard implementation, so there is no canonical id to
  // compare against — the market's choice is recorded on `Pool` and becomes the
  // pin for every later gate, exactly as with `feed_program` and `rate_program`.
  //
  // What that moves, and what it does not:
  //   * a creator can now bind their own market to a guard program of their
  //     choosing, including one that rubber-stamps. So could they always, by
  //     choosing a whitelist they control; the difference is that the program is
  //     now recorded and inspectable rather than assumed.
  //   * nobody else can substitute a program afterwards, which is the property
  //     that actually protects depositors.
  it("records the guard program the market chose", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    // `create` checks the creator against the list they chose, so they have to
    // be on it before the market can be stood up.
    await guardProgram.methods
      .add(poolAuthority.publicKey)
      .accounts({ guardState: guardPda, authority: guardAuthority.publicKey } as any)
      .signers([guardAuthority])
      .rpc();

    const pool = await createPool(poolAuthority, {
      program: guardProgram.programId,
      state: guardPda,
    });

    const account = await calmaProgram.account.pool.fetch(pool);
    expect(account.guardProgram.toString()).to.equal(guardProgram.programId.toString());
    expect(account.guardState.toString()).to.equal(guardPda.toString());
  });

  it("leaves guard_program zeroed on an ungated market", async () => {
    const poolAuthority = Keypair.generate();
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(poolAuthority.publicKey, LAMPORTS_PER_SOL)
    );

    const pool = await createPool(poolAuthority, null);

    const account = await calmaProgram.account.pool.fetch(pool);
    expect(account.guardProgram.toString()).to.equal(PublicKey.default.toString());
  });
});

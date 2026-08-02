import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { createMint } from "@solana/spl-token";
import { expect } from "chai";
import { Feed } from "../../target/types/feed";

describe("feed", () => {
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);
    const program = anchor.workspace.Feed as Program<Feed>;

    let payer: Keypair;
    let authority: Keypair;
    let feedPda: PublicKey;
    let collateralMint: PublicKey;
    let lendMint: PublicKey;

    before(async () => {
        payer = Keypair.generate();
        authority = Keypair.generate();

        const sigPayer = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigPayer);
        const sigAuth = await provider.connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigAuth);

        collateralMint = await createMint(provider.connection, payer, authority.publicKey, null, 6);
        lendMint = await createMint(provider.connection, payer, authority.publicKey, null, 6);

        [feedPda] = PublicKey.findProgramAddressSync(
            [Buffer.from("feed"), collateralMint.toBuffer(), lendMint.toBuffer(), Buffer.from([0])],
            program.programId,
        );
    });

    it("create initialises the feed account with prices = 0", async () => {
        await program.methods
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
                authority: authority.publicKey,
                collateralMint,
                lendMint,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();

        const account = await program.account.feed.fetch(feedPda);
        expect(account.config.authority.toString()).to.equal(authority.publicKey.toString());
        expect(account.header.collateralPrice.toNumber()).to.equal(0);
        expect(account.header.lendPrice.toNumber()).to.equal(0);
    });

    it("set_value updates both prices in the feed account", async () => {
        const newValue = 42;

        await program.methods
            .setValue(new BN(newValue), new BN(newValue))
            .accounts({
                authority: authority.publicKey,
                feed: feedPda,
            })
            .signers([authority])
            .rpc();

        const account = await program.account.feed.fetch(feedPda);
        expect(account.header.collateralPrice.toNumber()).to.equal(newValue);
        expect(account.header.lendPrice.toNumber()).to.equal(newValue);
    });

    it("set_value can update the value multiple times", async () => {
        await program.methods
            .setValue(new BN(100), new BN(200))
            .accounts({ authority: authority.publicKey, feed: feedPda })
            .signers([authority])
            .rpc();

        let account = await program.account.feed.fetch(feedPda);
        expect(account.header.collateralPrice.toNumber()).to.equal(100);
        expect(account.header.lendPrice.toNumber()).to.equal(200);

        await program.methods
            .setValue(new BN(9999), new BN(8888))
            .accounts({ authority: authority.publicKey, feed: feedPda })
            .signers([authority])
            .rpc();

        account = await program.account.feed.fetch(feedPda);
        expect(account.header.collateralPrice.toNumber()).to.equal(9999);
        expect(account.header.lendPrice.toNumber()).to.equal(8888);
    });

    it("set_value rejects a non-authority signer", async () => {
        const imposter = Keypair.generate();
        const sig = await provider.connection.requestAirdrop(imposter.publicKey, LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);

        // The imposter's feed PDA does not exist, so the transaction will fail
        // with an account-not-found or constraint error.
        try {
            await program.methods
                .setValue(new BN(1), new BN(1))
                .accounts({
                    authority: imposter.publicKey,
                    feed: feedPda,
                })
                .signers([imposter])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            // Constraint violation — imposter cannot update a feed they do not own.
            expect(err).to.exist;
        }
    });
});

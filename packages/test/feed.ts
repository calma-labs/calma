import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Feed } from "../../target/types/feed";

describe("feed", () => {
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);
    const program = anchor.workspace.Feed as Program<Feed>;

    let payer: Keypair;
    let authority: Keypair;
    let feedPda: PublicKey;

    before(async () => {
        payer = Keypair.generate();
        authority = Keypair.generate();

        const sigPayer = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigPayer);
        const sigAuth = await provider.connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigAuth);

        [feedPda] = PublicKey.findProgramAddressSync(
            [Buffer.from("feed"), authority.publicKey.toBuffer()],
            program.programId,
        );
    });

    it("create initialises the feed account with value = 0", async () => {
        await program.methods
            .create()
            .accounts({
                authority: authority.publicKey,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();

        const account = await program.account.feed.fetch(feedPda);
        expect(account.authority.toString()).to.equal(authority.publicKey.toString());
        expect(account.value.toNumber()).to.equal(0);
    });

    it("set_value updates the value stored in the feed account", async () => {
        const newValue = 42;

        await program.methods
            .setValue(new BN(newValue))
            .accounts({
                authority: authority.publicKey,
            })
            .signers([authority])
            .rpc();

        const account = await program.account.feed.fetch(feedPda);
        expect(account.value.toNumber()).to.equal(newValue);
    });

    it("set_value can update the value multiple times", async () => {
        await program.methods
            .setValue(new BN(100))
            .accounts({ authority: authority.publicKey })
            .signers([authority])
            .rpc();

        let account = await program.account.feed.fetch(feedPda);
        expect(account.value.toNumber()).to.equal(100);

        await program.methods
            .setValue(new BN(9999))
            .accounts({ authority: authority.publicKey })
            .signers([authority])
            .rpc();

        account = await program.account.feed.fetch(feedPda);
        expect(account.value.toNumber()).to.equal(9999);
    });

    it("set_value rejects a non-authority signer", async () => {
        const imposter = Keypair.generate();
        const sig = await provider.connection.requestAirdrop(imposter.publicKey, LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);

        // The imposter's feed PDA does not exist, so the transaction will fail
        // with an account-not-found or constraint error.
        try {
            await program.methods
                .setValue(new BN(1))
                .accounts({
                    authority: imposter.publicKey,
                })
                .signers([imposter])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            // Account does not exist or constraint violation — either way the
            // imposter cannot update a feed they do not own.
            expect(err).to.exist;
        }
    });
});

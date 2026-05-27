import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";

describe("irm set_fee_curve", () => {
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);
    const program = anchor.workspace.Irm as Program<Irm>;

    let payer: Keypair;
    let authority: Keypair;
    let pool: PublicKey;
    let irmConfig: PublicKey;

    before(async () => {
        payer = Keypair.generate();
        authority = Keypair.generate();

        const sigPayer = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigPayer);
        const sigAuth = await provider.connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigAuth);

        pool = Keypair.generate().publicKey;
        [irmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool.toBuffer()],
            program.programId
        );

        await program.methods
            .initialize()
            .accounts({
                pool,
                authority: authority.publicKey,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();
    });

    it("authority can update a curve segment", async () => {
        await program.methods
            .setFeeCurve(0, { a: new BN(500), b: new BN(200), a2: new BN(0), kink: new BN(0), enabled: true })
            .accounts({
                irmState: irmConfig,
                authority: authority.publicKey,
            })
            .signers([authority])
            .rpc();

        const config = await program.account.irmConfig.fetch(irmConfig);
        const c0 = config.model.curves[0];
        expect(c0.a.toNumber()).to.equal(500);
        expect(c0.b.toNumber()).to.equal(200);
        expect(c0.enabled).to.not.equal(0);
    });

    it("authority can write a kinked curve to slot 1", async () => {
        await program.methods
            .setFeeCurve(1, { a: new BN(200), b: new BN(0), a2: new BN(2000), kink: new BN(8000), enabled: true })
            .accounts({
                irmState: irmConfig,
                authority: authority.publicKey,
            })
            .signers([authority])
            .rpc();

        const config = await program.account.irmConfig.fetch(irmConfig);
        const c1 = config.model.curves[1];
        expect(c1.a.toNumber()).to.equal(200);
        expect(c1.a2.toNumber()).to.equal(2000);
        expect(c1.kink.toNumber()).to.equal(8000);
        expect(c1.enabled).to.not.equal(0);
    });

    it("authority can disable a curve by setting enabled=false", async () => {
        await program.methods
            .setFeeCurve(1, { a: new BN(200), b: new BN(0), a2: new BN(2000), kink: new BN(8000), enabled: false })
            .accounts({
                irmState: irmConfig,
                authority: authority.publicKey,
            })
            .signers([authority])
            .rpc();

        const config = await program.account.irmConfig.fetch(irmConfig);
        expect(config.model.curves[1].enabled).to.equal(0);
    });

    it("rejects an out-of-range curve index", async () => {
        try {
            await program.methods
                .setFeeCurve(4, { a: new BN(0), b: new BN(100), a2: new BN(0), kink: new BN(0), enabled: true })
                .accounts({
                    irmState: irmConfig,
                    authority: authority.publicKey,
                })
                .signers([authority])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.include("InvalidCurveIndex");
        }
    });

    it("rejects a non-authority signer", async () => {
        const imposter = Keypair.generate();
        const sig = await provider.connection.requestAirdrop(imposter.publicKey, LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);

        try {
            await program.methods
                .setFeeCurve(0, { a: new BN(0), b: new BN(9999), a2: new BN(0), kink: new BN(0), enabled: true })
                .accounts({
                    irmState: irmConfig,
                    authority: imposter.publicKey,
                })
                .signers([imposter])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.match(/Unauthorized|constraint/i);
        }
    });
});

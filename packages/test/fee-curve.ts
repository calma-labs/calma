import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";

describe("irm set_fee_points", () => {
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
            .initialize([
                { utilBps: 0, rateBps: 50 },
                { utilBps: 9_500, rateBps: 450 },
                { utilBps: 10_000, rateBps: 1_000 },
            ])
            .accounts({
                pool,
                authority: authority.publicKey,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();
    });

    it("authority can replace the point list", async () => {
        await program.methods
            .setFeePoints([
                { utilBps: 0, rateBps: 100 },
                { utilBps: 5_000, rateBps: 400 },
                { utilBps: 10_000, rateBps: 1_500 },
            ])
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await program.account.irmState.fetch(irmConfig);
        expect(config.model.len).to.equal(3);
        expect(config.model.points[0].utilBps).to.equal(0);
        expect(config.model.points[0].rateBps).to.equal(100);
        expect(config.model.points[2].utilBps).to.equal(10_000);
        expect(config.model.points[2].rateBps).to.equal(1_500);
    });

    it("accepts the minimum 2-point curve", async () => {
        await program.methods
            .setFeePoints([
                { utilBps: 0, rateBps: 250 },
                { utilBps: 10_000, rateBps: 250 },
            ])
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await program.account.irmState.fetch(irmConfig);
        expect(config.model.len).to.equal(2);
    });

    it("rejects a 1-point curve", async () => {
        try {
            await program.methods
                .setFeePoints([{ utilBps: 0, rateBps: 500 }])
                .accounts({ irmState: irmConfig, authority: authority.publicKey })
                .signers([authority])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.include("InvalidPointList");
        }
    });

    it("rejects a curve whose first utilization is non-zero", async () => {
        try {
            await program.methods
                .setFeePoints([
                    { utilBps: 100, rateBps: 0 },
                    { utilBps: 10_000, rateBps: 500 },
                ])
                .accounts({ irmState: irmConfig, authority: authority.publicKey })
                .signers([authority])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.include("InvalidPointList");
        }
    });

    it("rejects non-monotonic utilizations", async () => {
        try {
            await program.methods
                .setFeePoints([
                    { utilBps: 0, rateBps: 0 },
                    { utilBps: 5_000, rateBps: 400 },
                    { utilBps: 5_000, rateBps: 800 },
                ])
                .accounts({ irmState: irmConfig, authority: authority.publicKey })
                .signers([authority])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.include("InvalidPointList");
        }
    });

    it("rejects a non-authority signer", async () => {
        const imposter = Keypair.generate();
        const sig = await provider.connection.requestAirdrop(imposter.publicKey, LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);

        try {
            await program.methods
                .setFeePoints([
                    { utilBps: 0, rateBps: 0 },
                    { utilBps: 10_000, rateBps: 999 },
                ])
                .accounts({ irmState: irmConfig, authority: imposter.publicKey })
                .signers([imposter])
                .rpc();
            expect.fail("expected transaction to fail");
        } catch (err: any) {
            expect(err.toString()).to.match(/Unauthorized|constraint/i);
        }
    });
});

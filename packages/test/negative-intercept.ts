import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";

function curveArg(a: number, b: number, enabled = true) {
    return { a: new BN(a), b: new BN(b), kink: new BN(0), a2: new BN(0), enabled };
}

describe("negative intercept", () => {
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);
    const irmProgram = anchor.workspace.Irm as Program<Irm>;

    let payer: Keypair;
    let authority: Keypair;
    let irmConfig: PublicKey;

    beforeEach(async () => {
        payer = Keypair.generate();
        authority = Keypair.generate();

        const sigPayer = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigPayer);
        const sigAuth = await provider.connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigAuth);

        const pool = Keypair.generate().publicKey;
        [irmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool.toBuffer()],
            irmProgram.programId
        );

        await irmProgram.methods
            .initialize()
            .accounts({ pool, authority: authority.publicKey, payer: payer.publicKey })
            .signers([payer, authority])
            .rpc();
    });

    it("sets negative b on curve 1 and reads it back correctly", async () => {
        // Curve 0: flat 200 bps
        await irmProgram.methods
            .setFeeCurve(0, curveArg(0, 200))
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        // Curve 1: 14000*(u/10000) - 11000, negative below u≈7857
        await irmProgram.methods
            .setFeeCurve(1, curveArg(14000, -11000))
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await irmProgram.account.irmConfig.fetch(irmConfig);
        expect(config.model.curves[0].b.toNumber()).to.equal(200);
        expect(config.model.curves[1].a.toNumber()).to.equal(14000);
        expect(config.model.curves[1].b.toNumber()).to.equal(-11000);
    });

    it("sets negative linear intercept on curve 0 and reads it back", async () => {
        // Curve 0: 1000*(u/10000) - 500, negative until u=5000
        await irmProgram.methods
            .setFeeCurve(0, curveArg(1000, -500))
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await irmProgram.account.irmConfig.fetch(irmConfig);
        expect(config.model.curves[0].a.toNumber()).to.equal(1000);
        expect(config.model.curves[0].b.toNumber()).to.equal(-500);
    });

    it("sets negative intercepts across multiple curves", async () => {
        await irmProgram.methods
            .setFeeCurve(0, curveArg(500, -200))
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        await irmProgram.methods
            .setFeeCurve(1, curveArg(2000, -1000))
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await irmProgram.account.irmConfig.fetch(irmConfig);
        expect(config.model.curves[0].b.toNumber()).to.equal(-200);
        expect(config.model.curves[1].b.toNumber()).to.equal(-1000);
    });
});

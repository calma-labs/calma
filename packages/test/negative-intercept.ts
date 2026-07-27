import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";

describe("piecewise linear model edge cases", () => {
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
            .initialize([
                { utilBps: 0, rateBps: 0 },
                { utilBps: 10_000, rateBps: 500 },
            ])
            .accounts({ pool, authority: authority.publicKey, payer: payer.publicKey })
            .signers([payer, authority])
            .rpc();
    });

    it("three-point curve stores and reports correct rates", async () => {
        await irmProgram.methods
            .setFeePoints([
                { utilBps: 0, rateBps: 50 },
                { utilBps: 9_500, rateBps: 450 },
                { utilBps: 10_000, rateBps: 1_000 },
            ])
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await irmProgram.account.irmState.fetch(irmConfig);
        expect(config.model.len).to.equal(3);
        expect(config.model.points[0].rateBps).to.equal(50);
        expect(config.model.points[1].rateBps).to.equal(450);
        expect(config.model.points[2].rateBps).to.equal(1_000);
    });

    it("four-point curve stores all points correctly", async () => {
        await irmProgram.methods
            .setFeePoints([
                { utilBps: 0, rateBps: 100 },
                { utilBps: 2_500, rateBps: 300 },
                { utilBps: 7_500, rateBps: 500 },
                { utilBps: 10_000, rateBps: 1_200 },
            ])
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const config = await irmProgram.account.irmState.fetch(irmConfig);
        expect(config.model.len).to.equal(4);
        expect(config.model.points[0].utilBps).to.equal(0);
        expect(config.model.points[0].rateBps).to.equal(100);
        expect(config.model.points[3].utilBps).to.equal(10_000);
        expect(config.model.points[3].rateBps).to.equal(1_200);
    });

    it("extrapolates above the last point", async () => {
        // Slope of last segment: (1000 - 500) / (10000 - 9500) = 1 bps/bp
        // At util 10500 we expect 1000 + (10500-10000)*1 = 1500
        await irmProgram.methods
            .setFeePoints([
                { utilBps: 0, rateBps: 0 },
                { utilBps: 9_500, rateBps: 500 },
                { utilBps: 10_000, rateBps: 1_000 },
            ])
            .accounts({ irmState: irmConfig, authority: authority.publicKey })
            .signers([authority])
            .rpc();

        const pool = Keypair.generate().publicKey;
        const [irmConfig2] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool.toBuffer()],
            irmProgram.programId
        );
        await irmProgram.methods
            .initialize([
                { utilBps: 0, rateBps: 0 },
                { utilBps: 9_500, rateBps: 500 },
                { utilBps: 10_000, rateBps: 1_000 },
            ])
            .accounts({ pool, authority: authority.publicKey, payer: payer.publicKey })
            .signers([payer, authority])
            .rpc();

        const result = await irmProgram.methods
            .borrowRate(new BN(10_500))
            .accounts({ pool })
            .simulate();

        const log = result.raw.find((l) => l.includes("irm::borrow_rate"));
        expect(log).to.match(/irm::borrow_rate utilization=10500 rate=1500/);
    });
});

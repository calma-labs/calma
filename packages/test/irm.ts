import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";

describe("irm initialize", () => {
    const provider = AnchorProvider.env();
    anchor.setProvider(provider);
    const program = anchor.workspace.Irm as Program<Irm>;

    let payer: Keypair;
    let pool: PublicKey;
    let irmConfig: PublicKey;

    before(async () => {
        payer = Keypair.generate();
        const sig = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);

        pool = Keypair.generate().publicKey;
        [irmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool.toBuffer()],
            program.programId
        );
    });

    it("initializes irm config with correct a, b, and pool", async () => {
        const a = new BN(1000);
        const b = new BN(500);

        await program.methods
            .initialize(a, b)
            .accounts({
                irmConfig,
                pool,
                payer: payer.publicKey,
                systemProgram: SystemProgram.programId,
            })
            .signers([payer])
            .rpc();

        const config = await program.account.irmConfig.fetch(irmConfig);
        expect(config.a.toNumber()).to.equal(1000);
        expect(config.b.toNumber()).to.equal(500);
        expect(config.pool.toString()).to.equal(pool.toString());
    });

    it("different pools produce different config PDAs", async () => {
        const pool2 = Keypair.generate().publicKey;
        const [irmConfig2] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool2.toBuffer()],
            program.programId
        );

        await program.methods
            .initialize(new BN(200), new BN(300))
            .accounts({
                irmConfig: irmConfig2,
                pool: pool2,
                payer: payer.publicKey,
                systemProgram: SystemProgram.programId,
            })
            .signers([payer])
            .rpc();

        expect(irmConfig.toString()).to.not.equal(irmConfig2.toString());

        const config2 = await program.account.irmConfig.fetch(irmConfig2);
        expect(config2.pool.toString()).to.equal(pool2.toString());
    });
});

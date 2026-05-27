import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";
import { Jbl } from "../../target/types/jbl";
import { setupTest, participateInPool, TestSetup } from "./utils";

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
                pool,
                payer: payer.publicKey,
            })
            .signers([payer])
            .rpc();

        const config = await program.account.irmConfig.fetch(irmConfig);
        expect(config.a.toNumber()).to.equal(1000);
        expect(config.b.toNumber()).to.equal(500);
        expect(config.pool.toString()).to.equal(pool.toString());
    });

    it("calculates borrow rate correctly", async () => {
        // a=1000, b=500, utilization=5000 => 1000*5000/10000 + 500 = 1000
        const result = await program.methods
            .borrowRate(5000)
            .accounts({ irmState: irmConfig, pool })
            .simulate();

        const log = result.raw.find((l) => l.includes("irm::borrow_rate"));
        console.log("IRM log:", log);
        expect(log).to.match(/irm::borrow_rate utilization=5000 rate=1000/);
    });

    it("borrow rate via CPI from JBL borrow", async () => {
        const irmProgram = program;
        const poolKeypair = Keypair.generate();

        const [cpiIrmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), poolKeypair.publicKey.toBuffer()],
            irmProgram.programId
        );

        await irmProgram.methods
            .initialize(new BN(1000), new BN(500))
            .accounts({ pool: poolKeypair.publicKey, payer: payer.publicKey })
            .signers([payer])
            .rpc();

        const jblSetup: TestSetup = await setupTest(75, {
            poolKeypair,
            rateProgram: irmProgram.programId,
            rateState: cpiIrmConfig,
        });

        await participateInPool(jblSetup, 500_000_000);

        await jblSetup.program.methods
            .depositCollateral(new BN(100_000_000))
            .accounts({
                pool: jblSetup.pool,
                collateralMint: jblSetup.collateralMint,
                authority: jblSetup.authority.publicKey,
                userTokenAccount: jblSetup.userCollateralTokenAccount,
            })
            .signers([jblSetup.authority])
            .rpc();

        const sig = await jblSetup.program.methods
            .borrow(new BN(50_000_000))
            .accounts({
                pool: jblSetup.pool,
                lendMint: jblSetup.lendMint,
                authority: jblSetup.authority.publicKey,
            })
            .remainingAccounts([
                { pubkey: irmProgram.programId, isWritable: false, isSigner: false },
                { pubkey: cpiIrmConfig, isWritable: false, isSigner: false },
            ])
            .signers([jblSetup.authority])
            .rpc();

        const tx = await jblSetup.connection.getTransaction(sig, {
            commitment: "confirmed",
            maxSupportedTransactionVersion: 0,
        });
        const logs = tx?.meta?.logMessages ?? [];
        const log = logs.find((l) => l.includes("irm::borrow_rate"));
        console.log("IRM CPI log:", log);
        expect(log).to.match(/irm::borrow_rate utilization=\d+ rate=\d+/);
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
                pool: pool2,
                payer: payer.publicKey,
            })
            .signers([payer])
            .rpc();

        expect(irmConfig.toString()).to.not.equal(irmConfig2.toString());

        const config2 = await program.account.irmConfig.fetch(irmConfig2);
        expect(config2.pool.toString()).to.equal(pool2.toString());
    });
});

import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import { Irm } from "../../target/types/irm";
import { setupTest, participateInPool, TestSetup } from "./utils";

const FLAT_100_BPS = [
    { utilBps: 0, rateBps: 100 },
    { utilBps: 10_000, rateBps: 100 },
];

describe("irm initialize", () => {
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
        const sig = await provider.connection.requestAirdrop(payer.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sig);
        const sigAuth = await provider.connection.requestAirdrop(authority.publicKey, 2 * LAMPORTS_PER_SOL);
        await provider.connection.confirmTransaction(sigAuth);

        pool = Keypair.generate().publicKey;
        [irmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), pool.toBuffer()],
            program.programId
        );
    });

    it("initializes irm config with flat 100 bps curve", async () => {
        await program.methods
            .initialize(FLAT_100_BPS)
            .accounts({
                pool,
                authority: authority.publicKey,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();

        const config = await program.account.irmState.fetch(irmConfig);
        expect(config.pool.toString()).to.equal(pool.toString());
        expect(config.model.points[0].utilBps).to.equal(0);
        expect(config.model.points[0].rateBps).to.equal(100);
        expect(config.model.points[1].utilBps).to.equal(10_000);
        expect(config.model.points[1].rateBps).to.equal(100);
        expect(config.model.len).to.equal(2);
    });

    it("calculates borrow rate from flat 100 bps curve", async () => {
        const result = await program.methods
            .borrowRate(new BN(5000))
            .accounts({ pool })
            .simulate();

        const log = result.raw.find((l) => l.includes("irm::borrow_rate"));
        console.log("IRM log:", log);
        expect(log).to.match(/irm::borrow_rate utilization=5000 rate=100/);
    });

    it("borrow rate via CPI from JBL borrow", async () => {
        const irmProgram = program;
        const poolKeypair = Keypair.generate();

        const [cpiIrmConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("irm_config"), poolKeypair.publicKey.toBuffer()],
            irmProgram.programId
        );

        await irmProgram.methods
            .initialize(FLAT_100_BPS)
            .accounts({ pool: poolKeypair.publicKey, authority: authority.publicKey, payer: payer.publicKey })
            .signers([payer, authority])
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
                rateProgram: irmProgram.programId,
                irmState: cpiIrmConfig,
                feedProgram: jblSetup.feedProgram.programId,
                feedState: jblSetup.feedPda,
            })
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
            .initialize(FLAT_100_BPS)
            .accounts({
                pool: pool2,
                authority: authority.publicKey,
                payer: payer.publicKey,
            })
            .signers([payer, authority])
            .rpc();

        expect(irmConfig.toString()).to.not.equal(irmConfig2.toString());

        const config2 = await program.account.irmState.fetch(irmConfig2);
        expect(config2.pool.toString()).to.equal(pool2.toString());
    });
});

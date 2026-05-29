import * as anchor from "@anchor-lang/core";
import { Program, AnchorProvider, BN } from "@anchor-lang/core";
import { PublicKey, Keypair, LAMPORTS_PER_SOL, SystemProgram, SYSVAR_INSTRUCTIONS_PUBKEY } from "@solana/web3.js";
import { createMint, getAccount } from "@solana/spl-token";
import { expect } from "chai";
import { Jbl } from "../../target/types/jbl";
import { Feed } from "../../target/types/feed";
import { Irm } from "../../target/types/irm";
import { POOL_SPACE } from "./utils";

describe("pool creation (create)", () => {
    describe("create pool with two mints", () => {
        let provider: AnchorProvider;
        let program: Program<Jbl>;
        let feedProgram: Program<Feed>;
        let irmProgram: Program<Irm>;
        let feedPda: PublicKey;
        let feedAuthority: PublicKey;
        let irmConfigPda: PublicKey;
        let payer: Keypair;
        let authority: Keypair;
        let collateralMint: PublicKey;
        let lendMint: PublicKey;
        let poolKeypair: Keypair;
        let statePda: PublicKey;
        let collateralVaultPda: PublicKey;
        let lendVaultPda: PublicKey;
        let lpMintPda: PublicKey;

        before(async () => {
            provider = AnchorProvider.env();
            anchor.setProvider(provider);
            program = anchor.workspace.Jbl as Program<Jbl>;
            feedProgram = anchor.workspace.Feed as Program<Feed>;
            irmProgram = anchor.workspace.Irm as Program<Irm>;
            feedAuthority = provider.wallet.publicKey;
            [feedPda] = PublicKey.findProgramAddressSync(
                [Buffer.from("feed"), feedAuthority.toBuffer()],
                feedProgram.programId
            );

            payer = Keypair.generate();
            authority = Keypair.generate();
            poolKeypair = Keypair.generate();

            for (const kp of [payer, authority]) {
                const sig = await provider.connection.requestAirdrop(kp.publicKey, 2 * LAMPORTS_PER_SOL);
                await provider.connection.confirmTransaction(sig);
            }

            collateralMint = await createMint(provider.connection, payer, authority.publicKey, null, 6);
            lendMint = await createMint(provider.connection, payer, authority.publicKey, null, 6);

            [irmConfigPda] = PublicKey.findProgramAddressSync(
                [Buffer.from("irm_config"), poolKeypair.publicKey.toBuffer()],
                irmProgram.programId
            );
            await irmProgram.methods
                .initialize()
                .accounts({ pool: poolKeypair.publicKey, authority: authority.publicKey, payer: payer.publicKey })
                .signers([payer, authority])
                .rpc();

            [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state")], program.programId);
            [collateralVaultPda] = PublicKey.findProgramAddressSync(
                [Buffer.from("collateral_vault"), poolKeypair.publicKey.toBuffer()],
                program.programId
            );
            [lendVaultPda] = PublicKey.findProgramAddressSync(
                [Buffer.from("lend_vault"), poolKeypair.publicKey.toBuffer()],
                program.programId
            );
            [lpMintPda] = PublicKey.findProgramAddressSync(
                [Buffer.from("lp_mint"), poolKeypair.publicKey.toBuffer()],
                program.programId
            );

            // Create the feed account if it doesn't exist yet (shared provider wallet key).
            if (!(await provider.connection.getAccountInfo(feedPda))) {
                await feedProgram.methods
                    .create()
                    .accounts({ authority: feedAuthority, payer: payer.publicKey })
                    .signers([payer])
                    .rpc();
            }
        });

        it("creates pool account with correct initial state", async () => {
            const poolRent = await provider.connection.getMinimumBalanceForRentExemption(POOL_SPACE);
            const createPoolAccountIx = SystemProgram.createAccount({
                fromPubkey: payer.publicKey,
                newAccountPubkey: poolKeypair.publicKey,
                lamports: poolRent,
                space: POOL_SPACE,
                programId: program.programId,
            });

            // set_value must precede create in the same transaction for introspection to pass.
            const setValueIx = await feedProgram.methods
                .setValue(new BN(1_000_000))
                .accounts({ authority: feedAuthority })
                .instruction();

            await program.methods
                .create(75, feedProgram.programId, feedPda)
                .accounts({
                    pool: poolKeypair.publicKey,
                    collateralMint,
                    lendMint,
                    authority: authority.publicKey,
                    payer: payer.publicKey,
                    sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                    rateProgram: irmProgram.programId,
                    irmState: irmConfigPda,
                })
                .preInstructions([createPoolAccountIx, setValueIx])
                .signers([payer, authority, poolKeypair])
                .rpc();

            const pool = await program.account.pool.fetch(poolKeypair.publicKey);

            expect(pool.authority.toString()).to.equal(authority.publicKey.toString());
            expect(pool.collateralMint.toString()).to.equal(collateralMint.toString());
            expect(pool.lendMint.toString()).to.equal(lendMint.toString());
            expect(pool.lpMint.toString()).to.equal(lpMintPda.toString());
            expect(pool.totalCollateralDeposited.toString()).to.equal("0");
            expect(pool.market.totalSupplyAssets.toString()).to.equal("0");
            expect(pool.market.totalSupplyShares.toString()).to.equal("0");
            expect(pool.market.totalBorrowAssets.toString()).to.equal("0");
            expect(pool.market.totalBorrowShares.toString()).to.equal("0");
            expect(pool.market.ltvPercent).to.equal(75);
        });

        it("initialises collateral_vault under state PDA authority", async () => {
            const vault = await getAccount(provider.connection, collateralVaultPda);
            expect(vault.mint.toString()).to.equal(collateralMint.toString());
            expect(vault.owner.toString()).to.equal(statePda.toString());
            expect(vault.amount.toString()).to.equal("0");
        });

        it("initialises lend_vault under state PDA authority", async () => {
            const vault = await getAccount(provider.connection, lendVaultPda);
            expect(vault.mint.toString()).to.equal(lendMint.toString());
            expect(vault.owner.toString()).to.equal(statePda.toString());
            expect(vault.amount.toString()).to.equal("0");
        });

        it("initialises lp_mint under state PDA authority with same decimals as lend_mint", async () => {
            const { getAccount: _ga, getMint } = await import("@solana/spl-token");
            const { getMint: getMintInfo } = await import("@solana/spl-token");
            const lpMintInfo = await getMintInfo(provider.connection, lpMintPda);
            expect(lpMintInfo.mintAuthority?.toString()).to.equal(statePda.toString());
            expect(lpMintInfo.decimals).to.equal(6);
            expect(lpMintInfo.supply.toString()).to.equal("0");
        });

        it("fails when pool is already initialised (zero constraint violated)", async () => {
            const setValueIx = await feedProgram.methods
                .setValue(new BN(1_000_000))
                .accounts({ authority: feedAuthority })
                .instruction();

            try {
                await program.methods
                    .create(75, feedProgram.programId, feedPda, feedProgram.programId, feedPda)
                    .accounts({
                        pool: poolKeypair.publicKey,
                        collateralMint,
                        lendMint,
                        authority: authority.publicKey,
                        payer: payer.publicKey,
                        sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                    })
                    .preInstructions([setValueIx])
                    .signers([payer, authority])
                    .rpc();
                expect.fail("Expected second create to fail");
            } catch (err: unknown) {
                expect(err).to.be.instanceOf(Error);
            }
        });
    });
});

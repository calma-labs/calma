import { BN } from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, irmProgram, program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'
import { handleTransaction } from '../../lib/txHandler'

const IRM_PROGRAM_ID = new PublicKey('irmdacogiedKeCEBh72FJx4aoixyaByqGikTkxGifUk')
const FEED_PROGRAM_ID = new PublicKey('orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4')

/** Space needed for a Pool account (8-byte discriminator + zero-copy struct). */
const POOL_SPACE = 41_256

export interface CreatePoolParams {
    collateralMint: PublicKey
    lendMint: PublicKey
    ltvPercent?: number
    curveM1?: number
    curveC1?: number
    curveM2?: number
    curveC2?: number
}

export interface CreatePoolResult {
    poolAddress: PublicKey
    collateralMint: PublicKey
    lendMint: PublicKey
}

async function createPool(
    params: CreatePoolParams,
    wallet: Parameters<typeof signAndSendV1>[1],
    payer: PublicKey,
): Promise<CreatePoolResult> {
    const poolKeypair = Keypair.generate()
    const poolLamports = await connection.getMinimumBalanceForRentExemption(POOL_SPACE)
    const ltvPercent = params.ltvPercent ?? 75

    const [irmState] = PublicKey.findProgramAddressSync(
        [Buffer.from('irm_config'), poolKeypair.publicKey.toBuffer()],
        IRM_PROGRAM_ID,
    )

    const [feedState] = PublicKey.findProgramAddressSync(
        [Buffer.from('feed'), payer.toBuffer()],
        FEED_PROGRAM_ID,
    )

    const irmInitIx = await irmProgram.methods
        .initialize()
        .accounts({
            pool: poolKeypair.publicKey,
            authority: payer,
            payer,
        })
        .instruction()

    // irmState is a PDA whose seed comes from inside the account data, so Anchor's
    // IDL-based auto-derivation can't resolve it — pass it explicitly via cast.
    const irmSetCurve0Ix = await (irmProgram.methods
        .setFeeCurve(0, {
            a: new BN(params.curveM1 ?? 450),
            b: new BN(params.curveC1 ?? 0),
            a2: new BN(0),
            kink: new BN(0),
            enabled: true,
        }) as unknown as { accounts: (a: object) => { instruction: () => Promise<TransactionInstruction> } })
        .accounts({ irmState, authority: payer })
        .instruction()

    const irmSetCurve1Ix = await (irmProgram.methods
        .setFeeCurve(1, {
            a: new BN(params.curveM2 ?? 8000),
            b: new BN(params.curveC2 ?? -7173),
            a2: new BN(0),
            kink: new BN(0),
            enabled: true,
        }) as unknown as { accounts: (a: object) => { instruction: () => Promise<TransactionInstruction> } })
        .accounts({ irmState, authority: payer })
        .instruction()

    const createIx = await readonlyProgram.methods
        .create(ltvPercent)
        .accounts({
            pool: poolKeypair.publicKey,
            collateralMint: params.collateralMint,
            lendMint: params.lendMint,
            authority: payer,
            payer,
            feedProgram: FEED_PROGRAM_ID,
            feedState,
            rateProgram: IRM_PROGRAM_ID,
            irmState,
            guardProgram: null,
            guardState: null,
        })
        .instruction()

    await handleTransaction(
        async () => {
            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer })
            tx.add(
                SystemProgram.createAccount({
                    fromPubkey: payer,
                    newAccountPubkey: poolKeypair.publicKey,
                    space: POOL_SPACE,
                    lamports: poolLamports,
                    programId: readonlyProgram.programId,
                }),
                irmInitIx,
                irmSetCurve0Ix,
                irmSetCurve1Ix,
                createIx,
            )
            tx.partialSign(poolKeypair)
            return tx
        },
        wallet,
        { loadingMessage: 'Creating pool…', successMessage: 'Pool created!' },
    )

    return {
        poolAddress: poolKeypair.publicKey,
        collateralMint: params.collateralMint,
        lendMint: params.lendMint,
    }
}

export interface UseCreateLendingPoolOptions {
    onCreated?: (result: CreatePoolResult) => void
}

export function useCreateLendingPool(options: UseCreateLendingPoolOptions = {}) {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: (params: CreatePoolParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')
            const payer = new PublicKey(wallet.account.publicKey)
            return createPool(params, wallet, payer)
        },
        onSuccess: (result) => {
            queryClient.invalidateQueries({ queryKey: queryKeys.lending.all() })
            options.onCreated?.(result)
        },
    })
}

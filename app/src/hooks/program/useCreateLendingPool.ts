import { pool_space } from '@calma/wasm-lib'
import { useWalletConnection } from '@solana/react-hooks'
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, feedPda, IRM_PROGRAM_ID, irmProgram, program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'
import { handleTransaction } from '../../lib/txHandler'

/** Space needed for a Pool account (8-byte discriminator + zero-copy struct),
 * read from the Rust struct rather than copied. "Must stay in sync" is not a
 * mechanism: the copy this replaced sat 248 bytes below the real size, which
 * under-allocates the account and makes `create` fail. */
const POOL_SPACE = pool_space()

export interface IrmRatePointInput {
    /** Utilization in basis points (0..=10_000). First point must be 0. */
    utilBps: number
    /** Borrow rate in basis points. */
    rateBps: number
}

export interface CreatePoolParams {
    collateralMint: PublicKey
    lendMint: PublicKey
    ltvPercent?: number
    /** 2..=4 rate curve points. Defaults to the on-chain `DEFAULT_POINTS` if omitted. */
    ratePoints?: IrmRatePointInput[]
}

const DEFAULT_RATE_POINTS: IrmRatePointInput[] = [
    { utilBps: 0, rateBps: 50 },
    { utilBps: 9500, rateBps: 450 },
    { utilBps: 10000, rateBps: 1000 },
]

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

    const feedState = feedPda(params.collateralMint, params.lendMint)

    const ratePoints = (params.ratePoints ?? DEFAULT_RATE_POINTS).map((p) => ({
        utilBps: p.utilBps,
        rateBps: p.rateBps,
    }))

    const irmInitIx = await irmProgram.methods
        .initialize(ratePoints)
        .accounts({
            pool: poolKeypair.publicKey,
            authority: payer,
            payer,
        })
        .instruction()

    const createIx = await readonlyProgram.methods
        .create(ltvPercent)
        .accounts({
            pool: poolKeypair.publicKey,
            collateralMint: params.collateralMint,
            lendMint: params.lendMint,
            authority: payer,
            payer,
            feedState,
            rateProgram: IRM_PROGRAM_ID,
            irmState,
            // Genuinely a choice here, unlike on the entry instructions: `null`
            // creates an ungated market. Supplying a pair would bind the market
            // to that whitelist for life. The UI only creates open markets.
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

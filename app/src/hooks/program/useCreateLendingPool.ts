import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import {
    createInitializeMint2Instruction,
    getMinimumBalanceForRentExemptMint,
    MINT_SIZE,
    TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { signAndSendV1 } from '../../lib/transactions'
import { handleTransaction } from '../../lib/txHandler'
import { MINTER_KEYPAIR } from '../../store/wallet.store'

/** Decimal precision for auto-generated mints. */
const MINT_DECIMALS = 6

/** Space needed for a Pool account (8-byte discriminator + zero-copy struct). */
const POOL_SPACE = 41_256

export interface CreatePoolParams {
    ltvPercent?: number
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
    const collateralMintKeypair = Keypair.generate()
    const lendMintKeypair = Keypair.generate()
    const poolKeypair = Keypair.generate()

    // Step 1: create and initialize both SPL mints in a single transaction
    const mintLamports = await getMinimumBalanceForRentExemptMint(connection)

    await handleTransaction(
        async () => {
            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer })
            tx.add(
                SystemProgram.createAccount({
                    fromPubkey: payer,
                    newAccountPubkey: collateralMintKeypair.publicKey,
                    space: MINT_SIZE,
                    lamports: mintLamports,
                    programId: TOKEN_PROGRAM_ID,
                }),
                createInitializeMint2Instruction(
                    collateralMintKeypair.publicKey,
                    MINT_DECIMALS,
                    MINTER_KEYPAIR.publicKey,
                    null,
                ),
                SystemProgram.createAccount({
                    fromPubkey: payer,
                    newAccountPubkey: lendMintKeypair.publicKey,
                    space: MINT_SIZE,
                    lamports: mintLamports,
                    programId: TOKEN_PROGRAM_ID,
                }),
                createInitializeMint2Instruction(
                    lendMintKeypair.publicKey,
                    MINT_DECIMALS,
                    MINTER_KEYPAIR.publicKey,
                    null,
                ),
            )
            tx.partialSign(collateralMintKeypair, lendMintKeypair)
            return tx
        },
        wallet,
        { loadingMessage: 'Creating token mints…', successMessage: 'Mints created' },
    )

    // Step 2: pre-allocate pool account + initialize the pool in a single transaction
    const poolLamports = await connection.getMinimumBalanceForRentExemption(POOL_SPACE)
    const ltvPercent = params.ltvPercent ?? 75

    const createIx = await readonlyProgram.methods
        .create(ltvPercent)
        .accounts({
            pool: poolKeypair.publicKey,
            collateralMint: collateralMintKeypair.publicKey,
            lendMint: lendMintKeypair.publicKey,
            authority: payer,
            payer,
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
        collateralMint: collateralMintKeypair.publicKey,
        lendMint: lendMintKeypair.publicKey,
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

import { useWalletConnection } from '@solana/react-hooks'
import {
    createInitializeMint2Instruction,
    getMinimumBalanceForRentExemptMint,
    MINT_SIZE,
    TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js'
import { useMutation } from '@tanstack/react-query'
import { connection } from '../../lib/program'
import { signAndSendV1 } from '../../lib/transactions'
import { handleTransaction } from '../../lib/txHandler'
import { MINTER_KEYPAIR } from '../../store/wallet.store'

export interface CreateMintParams {
    decimals: number
    mintKeypair: Keypair
}

export interface CreateMintResult {
    mint: PublicKey
}

async function createMint(
    params: CreateMintParams,
    wallet: Parameters<typeof signAndSendV1>[1],
    payer: PublicKey,
): Promise<CreateMintResult> {
    const { mintKeypair } = params
    const lamports = await getMinimumBalanceForRentExemptMint(connection)

    await handleTransaction(
        async () => {
            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer })
            tx.add(
                SystemProgram.createAccount({
                    fromPubkey: payer,
                    newAccountPubkey: mintKeypair.publicKey,
                    space: MINT_SIZE,
                    lamports,
                    programId: TOKEN_PROGRAM_ID,
                }),
                createInitializeMint2Instruction(
                    mintKeypair.publicKey,
                    params.decimals,
                    MINTER_KEYPAIR.publicKey,
                    null,
                ),
            )
            tx.partialSign(mintKeypair)
            return tx
        },
        wallet,
        { loadingMessage: 'Creating mint…', successMessage: 'Mint created' },
    )

    return { mint: mintKeypair.publicKey }
}

export function useCreateMint() {
    const { connected, wallet } = useWalletConnection()

    return useMutation({
        mutationFn: (params: CreateMintParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')
            const payer = new PublicKey(wallet.account.publicKey)
            return createMint(params, wallet, payer)
        },
    })
}

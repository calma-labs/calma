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

export interface CreateMintsParams {
    decimals: number
    mintKeypairs: Keypair[]
}

async function createMints(
    params: CreateMintsParams,
    wallet: Parameters<typeof signAndSendV1>[1],
    payer: PublicKey,
): Promise<CreateMintResult[]> {
    const { mintKeypairs, decimals } = params
    const lamports = await getMinimumBalanceForRentExemptMint(connection)

    await handleTransaction(
        async () => {
            const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
            const tx = new Transaction({ blockhash, lastValidBlockHeight, feePayer: payer })
            for (const kp of mintKeypairs) {
                tx.add(
                    SystemProgram.createAccount({
                        fromPubkey: payer,
                        newAccountPubkey: kp.publicKey,
                        space: MINT_SIZE,
                        lamports,
                        programId: TOKEN_PROGRAM_ID,
                    }),
                    createInitializeMint2Instruction(kp.publicKey, decimals, MINTER_KEYPAIR.publicKey, null),
                )
            }
            for (const kp of mintKeypairs) tx.partialSign(kp)
            return tx
        },
        wallet,
        {
            loadingMessage: mintKeypairs.length > 1 ? `Creating ${mintKeypairs.length} mints…` : 'Creating mint…',
            successMessage: mintKeypairs.length > 1 ? `${mintKeypairs.length} mints created` : 'Mint created',
        },
    )

    return mintKeypairs.map(kp => ({ mint: kp.publicKey }))
}

export function useCreateMint() {
    const { connected, wallet } = useWalletConnection()

    return useMutation({
        mutationFn: ({ decimals, mintKeypair }: CreateMintParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')
            const payer = new PublicKey(wallet.account.publicKey)
            return createMints({ decimals, mintKeypairs: [mintKeypair] }, wallet, payer)
        },
    })
}

export function useCreateMints() {
    const { connected, wallet } = useWalletConnection()

    return useMutation({
        mutationFn: (params: CreateMintsParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')
            const payer = new PublicKey(wallet.account.publicKey)
            return createMints(params, wallet, payer)
        },
    })
}

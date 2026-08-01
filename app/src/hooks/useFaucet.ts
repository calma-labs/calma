import { useWalletConnection } from '@solana/react-hooks'
import { BN } from '@anchor-lang/core'
import { PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, faucetProgram } from '../lib/program'
import { queryKeys } from '../lib/queryKeys'
import { handleTransaction } from '../lib/txHandler'
import { useWalletBalancesStore } from '../store/wallet.store'

const FAUCET_AMOUNT = new BN(1_000_000_000) // 1 000 tokens at 6 decimals

/**
 * One `faucet.mint` instruction: the program signs the mint with its
 * `["mint_authority"]` PDA and creates the recipient's ATA if needed, so the
 * client contributes no signature beyond the wallet's own.
 */
function faucetMintIx(mint: PublicKey, recipient: PublicKey): Promise<TransactionInstruction> {
    return faucetProgram.methods
        .mint(FAUCET_AMOUNT)
        // mintAuthority and recipientTokenAccount are derived PDAs — Anchor resolves both.
        .accounts({
            payer: recipient,
            recipient,
            mint,
        })
        .instruction()
}

/**
 * Mutation hook for minting all provided test tokens in a single transaction.
 */
export function useFaucetAll(mints: PublicKey[]) {
    const { wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async () => {
            if (!wallet) throw new Error('Wallet not connected')
            if (mints.length === 0) throw new Error('No mints provided')

            const payer = new PublicKey(wallet.account.publicKey)

            const result = await handleTransaction(
                async () => {
                    const tx = new Transaction()
                    tx.feePayer = payer
                    const { blockhash } = await connection.getLatestBlockhash()
                    tx.recentBlockhash = blockhash

                    tx.add(...(await Promise.all(mints.map((mint) => faucetMintIx(mint, payer)))))

                    return tx
                },
                wallet,
                {
                    loadingMessage: 'Minting test tokens…',
                    successMessage: `${mints.length} tokens minted to your wallet`,
                    errorMessage: 'Faucet failed — mint is not owned by the faucet program',
                },
            )
            return result
        },
        onSuccess: () => {
            const address = wallet ? String(wallet.account.address) : null
            if (address) {
                queryClient.invalidateQueries({ queryKey: queryKeys.wallet.balances(address) })
                void useWalletBalancesStore.getState().fetch(address)
            }
        },
    })
}

/**
 * Mutation hook for minting test tokens to the connected wallet.
 * The faucet program mints under its own PDA authority — no client-held key.
 *
 * Automatically invalidates wallet balances on success so all balance
 * displays update without a manual refresh.
 */
export function useFaucet(mint: PublicKey) {
    const { wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async () => {
            if (!wallet) throw new Error('Wallet not connected')

            const payer = new PublicKey(wallet.account.publicKey)

            const result = await handleTransaction(
                async () => {
                    const tx = new Transaction()
                    tx.feePayer = payer

                    const { blockhash } = await connection.getLatestBlockhash()
                    tx.recentBlockhash = blockhash

                    tx.add(await faucetMintIx(mint, payer))
                    return tx
                },
                wallet,
                {
                    loadingMessage: 'Minting test tokens…',
                    successMessage: '1 000 tokens minted to your wallet',
                    errorMessage: 'Faucet failed — mint is not owned by the faucet program',
                },
            )
            return result
        },
        onSuccess: (_data, _vars, _ctx) => {
            // Refresh balances for the connected wallet
            const address = wallet ? String(wallet.account.address) : null
            if (address) {
                queryClient.invalidateQueries({
                    queryKey: queryKeys.wallet.balances(address),
                })
                void useWalletBalancesStore.getState().fetch(address)
            }
        },
    })
}

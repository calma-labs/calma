import { useMemo } from 'react'
import { useWalletConnection } from '@solana/react-hooks'
import { AnchorProvider, Program } from '@anchor-lang/core'
import { PublicKey, Transaction, VersionedTransaction } from '@solana/web3.js'
import { connection } from '../lib/program'
import { signV1WithSession } from '../lib/transactions'
import type { Calma } from '../../../target/types/calma'
import IDL from '../../../target/idl/calma.json'

/**
 * Returns an Anchor Program wired to the currently connected wallet.
 * Returns null when no wallet is connected or it lacks signTransaction.
 */
export function useAnchorProgram(): Program<Calma> | null {
    const { connected, wallet } = useWalletConnection()

    return useMemo(() => {
        if (!connected || !wallet?.signTransaction) return null

        const publicKey = new PublicKey(wallet.account.publicKey)

        const anchorWallet = {
            publicKey,
            signTransaction: <T extends Transaction | VersionedTransaction>(tx: T) =>
                signV1WithSession(tx, wallet),
            signAllTransactions: <T extends Transaction | VersionedTransaction>(txs: T[]): Promise<T[]> =>
                Promise.all(txs.map((tx) => signV1WithSession(tx, wallet))),
        }

        const provider = new AnchorProvider(connection, anchorWallet, { commitment: 'confirmed' })
        return new Program<Calma>(IDL as unknown as Calma, provider)
    }, [connected, wallet])
}

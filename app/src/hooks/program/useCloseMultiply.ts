import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, SYSVAR_INSTRUCTIONS_PUBKEY, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'
import { MINTER_KEYPAIR, useWalletBalancesStore } from '../../store/wallet.store'
import { flash_fee } from '@jbl/wasm-lib'

/** Flash fee via the shared wasm math (mirrors the on-chain fee exactly). */
function computeFlashFee(amount: anchor.BN): anchor.BN {
    return new anchor.BN((flash_fee(BigInt(amount.toString())) ?? 0n).toString())
}

export interface CloseMultiplyParams {
    pool: PublicKey
    lendMint: PublicKey
    collateralMint: PublicKey
    /** User's collateral ATA — receives withdrawn collateral; source for mock swap. */
    userCollateralAta: PublicKey
    /** User's lend ATA — receives flash-borrowed lend; source for flash repay. */
    userLendAta: PublicKey
    /** Raw debt amount to repay (derived from debtShares). */
    debtRaw: anchor.BN
    /** Raw collateral amount to withdraw (from userPosition.collateralDeposited). */
    collateralRaw: anchor.BN
}

/**
 * Close a multiply position via a flash-loan unwind.
 *
 * Transaction sequence:
 *   1. flashBorrow(debtRaw)                        — borrow lend to repay debt
 *   2. repay(debtRaw)                              — clear all debt
 *   3. withdrawCollateral(collateralRaw)            — reclaim collateral
 *   4. mockSwap(collateralRaw, collateral → lend)  — convert to lend for repay
 *   5. flashRepay(debtRaw + fee)                   — return flash loan
 *
 * Net result: user receives (collateralRaw − debtRaw − fee) lend tokens.
 * Requires positive equity (collateralRaw > debtRaw + fee).
 *
 * Note: if pool utilization is too high, withdrawCollateral may be queued
 * on-chain, causing the flash repay to fail. In that case the transaction
 * reverts atomically with no state change.
 */
export function useCloseMultiply() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            pool,
            lendMint,
            collateralMint,
            userCollateralAta,
            userLendAta,
            debtRaw,
            collateralRaw,
        }: CloseMultiplyParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)
            const flashFee = computeFlashFee(debtRaw)
            const flashRepayAmt = debtRaw.add(flashFee)

            const [flashBorrowIx, repayIx, withdrawIx, swapIx, flashRepayIx] =
                await Promise.all([
                    readonlyProgram.methods
                        .flashBorrow(debtRaw)
                        .accounts({
                            pool,
                            lendMint,
                            userDestination: userLendAta,
                            sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                        })
                        .instruction(),
                    // eslint-disable-next-line @typescript-eslint/no-explicit-any
                    readonlyProgram.methods.repay(debtRaw).accounts({ pool, lendMint, authority } as any).instruction(),
                    readonlyProgram.methods
                        .withdrawCollateral(collateralRaw)
                        // eslint-disable-next-line @typescript-eslint/no-explicit-any
                        .accounts({ pool, collateralMint, authority, userTokenAccount: userCollateralAta } as any)
                        .instruction(),
                    readonlyProgram.methods
                        .mockSwap(collateralRaw)
                        .accounts({
                            mintAuthority: MINTER_KEYPAIR.publicKey,
                            tokenOwner: authority,
                            mintIn: collateralMint,
                            mintOut: lendMint,
                            userTokenIn: userCollateralAta,
                            userTokenOut: userLendAta,
                        })
                        .instruction(),
                    readonlyProgram.methods
                        .flashRepay(flashRepayAmt)
                        .accounts({
                            pool,
                            lendMint,
                            userSource: userLendAta,
                            authority,
                            sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                        })
                        .instruction(),
                ])

            const tx = new Transaction().add(flashBorrowIx, repayIx, withdrawIx, swapIx, flashRepayIx)
            tx.feePayer = authority

            // Get blockhash first - needed before partialSign
            const { blockhash } = await connection.getLatestBlockhash()
            tx.recentBlockhash = blockhash

            // Sign with hardcoded minter before wallet signs (required for mockSwap)
            tx.partialSign(MINTER_KEYPAIR)

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Closing multiply position…', successMessage: 'Position closed!' },
            )
        },
        onSuccess: (_data, { pool }) => {
            const authority = wallet ? new PublicKey(wallet.account.publicKey) : null
            queryClient.invalidateQueries({ queryKey: queryKeys.lending.one(pool) })
            if (authority) {
                queryClient.invalidateQueries({ queryKey: queryKeys.userPosition.one(pool, authority) })
                void useWalletBalancesStore.getState().fetch(authority.toBase58())
            }
        },
    })
}

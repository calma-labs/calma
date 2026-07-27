import * as anchor from '@anchor-lang/core'
import { useWalletConnection } from '@solana/react-hooks'
import { PublicKey, SYSVAR_INSTRUCTIONS_PUBKEY, Transaction } from '@solana/web3.js'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { connection, FEED_PROGRAM_ID, IRM_PROGRAM_ID, irmStatePda, program as readonlyProgram } from '../../lib/program'
import { queryKeys } from '../../lib/queryKeys'
import { handleTransaction } from '../../lib/txHandler'
import { MINTER_KEYPAIR, useWalletBalancesStore } from '../../store/wallet.store'
import { flash_fee } from '@calma/wasm-lib'
import { refreshFeedForDevnet } from './refreshFeedForDevnet'

/** Flash fee via the shared wasm math (mirrors the on-chain fee exactly). */
function computeFlashFee(amount: anchor.BN): anchor.BN {
    return new anchor.BN((flash_fee(BigInt(amount.toString())) ?? 0n).toString())
}

export interface OpenMultiplyParams {
    pool: PublicKey
    lendMint: PublicKey
    collateralMint: PublicKey
    feedState: PublicKey
    /** User's collateral ATA — source of initial capital; receives swapped tokens. */
    userCollateralAta: PublicKey
    /** User's lend ATA — receives flash-borrowed lend tokens; source for flash repay. */
    userLendAta: PublicKey
    /** Own capital added this action (raw, no decimals). Deposited as collateral. */
    amountRaw: anchor.BN
    /** Leverage applied to this action's capital, e.g. 2.5 for 2.5×. */
    leverage: number
}

/**
 * Open (or add a leveraged tranche to) a multiply position via a flash-loan loop.
 *
 * Transaction sequence:
 *   1. depositCollateral(amount)              — user's own capital
 *   2. flashBorrow(extra = amount × (L−1))    — lend tokens
 *   3. mockSwap(extra, lend → collateral)     — synthetic 1:1 swap
 *   4. depositCollateral(extra)               — swapped collateral
 *   5. borrow(extra + fee)                    — lend tokens to cover flash repay
 *   6. flashRepay(extra + fee)
 *
 * Adds `amount × L` collateral and `amount × (L−1)` debt. All steps are additive
 * on-chain, so calling this against an existing position stacks another tranche;
 * each tranche's own LTV is `(L−1)/L ≤ pool LTV`, so the whole position stays
 * within LTV as long as `L ≤ 1/(1−LTV)` (see `PoolWithIrm::max_leverage`).
 *
 * The user pays zero lend tokens net — the flash fee is embedded in the borrow.
 * The user must hold `amountRaw` collateral tokens before calling this.
 */
export function useOpenMultiply() {
    const { connected, wallet } = useWalletConnection()
    const queryClient = useQueryClient()

    return useMutation({
        mutationFn: async ({
            pool,
            lendMint,
            collateralMint,
            feedState,
            userCollateralAta,
            userLendAta,
            amountRaw,
            leverage,
        }: OpenMultiplyParams) => {
            if (!connected || !wallet) throw new Error('Wallet not connected')

            const authority = new PublicKey(wallet.account.publicKey)

            await refreshFeedForDevnet(connection, pool)

            // extra = amount × (leverage − 1); use integer ×1000 to stay in BN arithmetic
            const leverageMilli = Math.round(leverage * 1_000)
            const extraRaw = amountRaw.muln(leverageMilli - 1_000).divn(1_000)
            const flashFee = computeFlashFee(extraRaw)
            const flashRepayAmt = extraRaw.add(flashFee)

            const [depositInitialIx, flashBorrowIx, swapIx, depositExtraIx, borrowIx, flashRepayIx] =
                await Promise.all([
                    readonlyProgram.methods
                        .depositCollateral(amountRaw)
                        .accounts({ pool, collateralMint, authority, userTokenAccount: userCollateralAta })
                        .instruction(),
                    readonlyProgram.methods
                        .flashBorrow(extraRaw)
                        .accounts({
                            pool,
                            lendMint,
                            userDestination: userLendAta,
                            sysvarInstructions: SYSVAR_INSTRUCTIONS_PUBKEY,
                        })
                        .instruction(),
                    readonlyProgram.methods
                        .mockSwap(extraRaw)
                        .accounts({
                            mintAuthority: MINTER_KEYPAIR.publicKey,
                            tokenOwner: authority,
                            mintIn: lendMint,
                            mintOut: collateralMint,
                            userTokenIn: userLendAta,
                            userTokenOut: userCollateralAta,
                        })
                        .instruction(),
                    readonlyProgram.methods
                        .depositCollateral(extraRaw)
                        .accounts({ pool, collateralMint, authority, userTokenAccount: userCollateralAta })
                        .instruction(),
                    // Borrow enough lend tokens to cover the flash repay
                    readonlyProgram.methods
                        .borrow(flashRepayAmt)
                        // eslint-disable-next-line @typescript-eslint/no-explicit-any
                        .accounts({ pool, lendMint, authority, rateProgram: IRM_PROGRAM_ID, irmState: irmStatePda(pool), feedProgram: FEED_PROGRAM_ID, feedState } as any)
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

            const tx = new Transaction().add(
                depositInitialIx,
                flashBorrowIx,
                swapIx,
                depositExtraIx,
                borrowIx,
                flashRepayIx,
            )
            tx.feePayer = authority

            // Get blockhash first - needed before partialSign
            const { blockhash } = await connection.getLatestBlockhash()
            tx.recentBlockhash = blockhash

            // Sign with hardcoded minter before wallet signs (required for mockSwap)
            tx.partialSign(MINTER_KEYPAIR)

            return handleTransaction(
                async () => tx,
                wallet,
                { loadingMessage: 'Opening multiply position…', successMessage: 'Position opened!' },
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

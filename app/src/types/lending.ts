import type { PublicKey } from '@solana/web3.js'

export interface UtilizationFeeConfig {
    m1: bigint
    c1: bigint
    m2: bigint
    c2: bigint
}

export interface PoolData {
    publicKey: PublicKey
    authority: PublicKey
    collateralMint: PublicKey
    lendMint: PublicKey
    lpMint: PublicKey
    totalCollateralDeposited: bigint
    totalLendDeposited: bigint
    totalBorrowed: bigint
    totalDebtShares: bigint
    lastAccrualTs: bigint
    totalLpIssued: bigint
    feeConfig: UtilizationFeeConfig
    ltvPercent: number
    lpMintBump: number
    /** Sum of all pending withdrawal amounts currently in the on-chain queue (raw token units). */
    pendingWithdrawals: bigint
}

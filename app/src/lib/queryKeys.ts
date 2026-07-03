import type { PublicKey } from '@solana/web3.js'

export const queryKeys = {
    lending: {
        all: () => ['lending', 'pools'] as const,
        one: (pool: PublicKey) => ['lending', 'pool', pool.toBase58()] as const,
    },
    userPosition: {
        all: () => ['user-positions'] as const,
        byPool: (pool: PublicKey) => ['user-positions', 'pool', pool.toBase58()] as const,
        byAuthority: (authority: PublicKey) =>
            ['user-positions', 'authority', authority.toBase58()] as const,
        one: (pool: PublicKey, authority: PublicKey) =>
            ['user-positions', pool.toBase58(), authority.toBase58()] as const,
    },
    feeds: {
        byPair: (collateralMint: string, lendMint: string) =>
            ['feeds', 'pair', collateralMint, lendMint] as const,
    },
    guard: {
        state: (authority: string) => ['guard', 'state', authority] as const,
    },
    wallet: {
        balances: (address: string) => ['wallet', 'balances', address] as const,
    },
    klines: {
        perp: (symbol: string, interval: string) =>
            ['klines', 'perp', symbol, interval] as const,
    },
} as const

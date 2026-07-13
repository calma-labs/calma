import type { Category } from '@/types/pool'

export const USDC_ICON = 'https://wsrv.nl/?w=64&h=64&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FEPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v%2Flogo.png&dpr=2&quality=80'
export const USDT_ICON = 'https://wsrv.nl/?w=64&h=64&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FEs9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB%2Flogo.svg&dpr=2&quality=80'

export interface TokenMeta {
    name: string
    symbol: string
    icon: string
    category: Category
    binancePerp?: string
}

const TOKENS: Record<string, TokenMeta> = {
    '7o6D8yQ5Lh6TtCHvSsba61katHH5yu9Uc3HNz6ud8jGZ': {
        name: 'Tether USD',
        symbol: 'USDT',
        icon: USDT_ICON,
        category: 'stablecoin',
    },
    '5ACU6KVLxkiWhnpk3XQ5buVHDzVKWskQZowVPJ3nL7Uk': {
        name: 'Tesla',
        symbol: 'TSLAx',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FTSLAx.png&dpr=2&quality=80',
        category: 'volatile',
        binancePerp: 'TSLAUSDT',
    },
    '9yrNzB6mLc4HTqA5jrZdAsWCbojyxDG6dNqAeFMGW4x8': {
        name: 'Nvidia',
        symbol: 'NVDAx',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FNVDAx.png&dpr=2&quality=80',
        category: 'volatile',
        binancePerp: 'NVDAUSDT',
    },
    '6cE4YR7WmMQJiN88gP3uEbDtdDVCQ9BMUFScNJFAqFpb': {
        name: 'Circle',
        symbol: 'CRCLx',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FCRCLx.png&dpr=2&quality=80',
        category: 'volatile',
        binancePerp: 'CRCLUSDT',
    },
    'CLG3cmT4c1hrBW7vUouhvDZ3fXhv7j5BvsLkAaf7js2V': {
        name: 'Marinade Staked SOL',
        symbol: 'mSOL',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FmSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So%2Flogo.png&dpr=2&quality=80',
        category: 'lsd',
        binancePerp: 'SOLUSDT',
    },
    'GhTVtceExcwpT4LN5QjNgFpgWBztQyjyq2KcsWyx6HYG': {
        name: 'Prime',
        symbol: 'PRIME',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fstorage.googleapis.com%2Fhastra-cdn-prod%2Fspl%2Fprimetoken.png&dpr=2&quality=80',
        category: 'volatile',
    },
    // USDC — used as collateral in multiple pools
    'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v': {
        name: 'USD Coin',
        symbol: 'USDC',
        icon: USDC_ICON,
        category: 'stablecoin',
    },
    // SOL — used as collateral in mSOL pool
    'So11111111111111111111111111111111111111112': {
        name: 'Solana',
        symbol: 'SOL',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FSo11111111111111111111111111111111111111112%2Flogo.png&dpr=2&quality=80',
        category: 'volatile',
        binancePerp: 'SOLUSDT',
    },
    // USDG — used as collateral in CRCLx pool
    'USDGkkjxMPfRpNpM74F8diNBYXnFkBD4iqiQaFXCYFN': {
        name: 'Global Dollar',
        symbol: 'USDG',
        icon: 'https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2F424565.fs1.hubspotusercontent-na1.net%2Fhubfs%2F424565%2FGDN-USDG-Token-512x512.png&dpr=2&quality=80',
        category: 'stablecoin',
    },
    // CASH — used as collateral in PRIME pool
    'CASHVDm2wsJXfhj6VWxb7GiMdoLc17Du7paH4bNr5woT': {
        name: 'Cash',
        symbol: 'CASH',
        icon: 'https://token-metadata.bridge.xyz/images/cash.png',
        category: 'stablecoin',
    },
}

/** Look up token metadata by mint (or any registered) address. Returns null when unknown. */
export function getTokenMeta(address: string): TokenMeta | null {
    return TOKENS[address] ?? null
}

/** All registered tokens, ordered as defined — used for iteration and dropdown options. */
export function getTokenOptions(): Array<{ address: string; symbol: string; icon: string }> {
    return Object.entries(TOKENS).map(([address, meta]) => ({
        address,
        symbol: meta.symbol,
        icon: meta.icon,
    }))
}

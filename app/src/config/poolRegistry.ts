import type { Category } from '@/types/pool'

/** Default icon shown when no token metadata is available. */
export const USDC_ICON = 'https://wsrv.nl/?w=64&h=64&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FEPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v%2Flogo.png&dpr=2&quality=80'
export const USDT_ICON = "https://wsrv.nl/?w=64&h=64&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FEs9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB%2Flogo.svg&dpr=2&quality=80"

/** Static metadata for a pool, keyed by the pool's on-chain address. */
export interface PoolMeta {
    name: string
    symbol: string
    icon: string
    collateralSymbol: string
    collateralIcon: string
    lendSymbol: string
    lendIcon: string
    category: Category
    /**
     * Binance USD-M perpetual futures symbol used for the price chart.
     * If undefined the chart falls back to generated mock data.
     * Example: 'TSLAUSDT', 'NVDAUSDT'
     */
    binancePerp?: string
}

const WHITELISTED_TOKENS: Record<string, PoolMeta> = {
    // ── Default / first entry ────────────────────────────────────────────────
    // Used as fallback when a pool address is not found in the registry.
    '7o6D8yQ5Lh6TtCHvSsba61katHH5yu9Uc3HNz6ud8jGZ': {
        name: 'Tether USD',
        symbol: 'USDT',
        icon: USDT_ICON,
        collateralSymbol: 'USDC',
        collateralIcon: USDC_ICON,
        lendSymbol: 'USDT',
        lendIcon: USDT_ICON,
        category: 'stablecoin',
    },
    '5ACU6KVLxkiWhnpk3XQ5buVHDzVKWskQZowVPJ3nL7Uk': {
        name: 'Tesla',
        symbol: 'TSLAx',
        icon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FTSLAx.png&dpr=2&quality=80",
        collateralSymbol: 'USDT',
        collateralIcon: USDT_ICON,
        lendSymbol: 'TSLAx',
        lendIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FTSLAx.png&dpr=2&quality=80",
        category: 'volatile',
        binancePerp: 'TSLAUSDT',
    },
    '9yrNzB6mLc4HTqA5jrZdAsWCbojyxDG6dNqAeFMGW4x8': {
        name: 'Nvidia',
        symbol: 'NVDAx',
        icon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FNVDAx.png&dpr=2&quality=80",
        collateralSymbol: 'USDC',
        collateralIcon: USDC_ICON,
        lendSymbol: 'NVDAx',
        lendIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FNVDAx.png&dpr=2&quality=80",
        category: 'volatile',
        binancePerp: 'NVDAUSDT',
    },
    "6cE4YR7WmMQJiN88gP3uEbDtdDVCQ9BMUFScNJFAqFpb": {
        name: 'Circle',
        symbol: 'CRCLx',
        icon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FCRCLx.png&dpr=2&quality=80",
        collateralSymbol: 'USDG',
        collateralIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2F424565.fs1.hubspotusercontent-na1.net%2Fhubfs%2F424565%2FGDN-USDG-Token-512x512.png&dpr=2&quality=80",
        lendSymbol: 'CRCLx',
        lendIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fxstocks-metadata.backed.fi%2Flogos%2Ftokens%2FCRCLx.png&dpr=2&quality=80",
        category: 'volatile',
        binancePerp: 'CRCLUSDT'
    },
    "CLG3cmT4c1hrBW7vUouhvDZ3fXhv7j5BvsLkAaf7js2V": {
        name: 'Marinade Staked SOL',
        symbol: 'mSOL',
        icon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FmSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So%2Flogo.png&dpr=2&quality=80",
        collateralSymbol: 'SOL',
        collateralIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FSo11111111111111111111111111111111111111112%2Flogo.png&dpr=2&quality=80",
        lendSymbol: 'mSOL',
        lendIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fraw.githubusercontent.com%2Fsolana-labs%2Ftoken-list%2Fmain%2Fassets%2Fmainnet%2FmSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So%2Flogo.png&dpr=2&quality=80",
        category: 'lsd',
        binancePerp: 'SOLUSDT',
    },
    "GhTVtceExcwpT4LN5QjNgFpgWBztQyjyq2KcsWyx6HYG": {
        name: "Prime",
        symbol: 'PRIME',
        icon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fstorage.googleapis.com%2Fhastra-cdn-prod%2Fspl%2Fprimetoken.png&dpr=2&quality=80",
        collateralSymbol: 'CASH',
        collateralIcon: "https://token-metadata.bridge.xyz/images/cash.png",
        lendSymbol: 'PRIME',
        lendIcon: "https://wsrv.nl/?w=32&h=32&url=https%3A%2F%2Fstorage.googleapis.com%2Fhastra-cdn-prod%2Fspl%2Fprimetoken.png&dpr=2&quality=80",
        category: 'volatile',
    },
}

/** The fallback metadata returned for any unrecognised pool address. */
const DEFAULT_POOL_META: PoolMeta = WHITELISTED_TOKENS["7o6D8yQ5Lh6TtCHvSsba61katHH5yu9Uc3HNz6ud8jGZ"]

/**
 * Look up display metadata for the given pool address.
 * Falls back to the default (USDT/USDC) entry if the address is not registered.
 */
export function getPoolMeta(address: string): PoolMeta {
    return WHITELISTED_TOKENS[address] || DEFAULT_POOL_META
}

/** All non-default entries from the whitelist, ordered as defined. */
export function getWhitelistedTokens(): Array<{ address: string; meta: PoolMeta }> {
    return Object.entries(WHITELISTED_TOKENS)
        .filter(([key]) => key !== 'DEFAULT')
        .map(([address, meta]) => ({ address, meta }))
}

/** Flat token list ready for use in a TokenSelect dropdown. */
export function getTokenOptions(): Array<{ address: string; symbol: string; icon: string }> {
    return getWhitelistedTokens().map(({ address, meta }) => ({
        address,
        symbol: meta.lendSymbol,
        icon: meta.lendIcon,
    }))
}

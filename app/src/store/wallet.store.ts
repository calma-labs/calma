import { TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID } from '@solana/spl-token'
import { Keypair, PublicKey } from '@solana/web3.js'
import { create } from 'zustand'
import { connection } from '../lib/program'
import type { TokenBalance, WalletBalances } from '../types/wallet'

// Hardcoded minter keypair for proof-of-concept faucet and mock swap
// This allows anyone to mint test tokens and use mock swap without being the mint authority
const MINTER_SECRET_KEY = new Uint8Array([164, 83, 220, 177, 59, 188, 88, 49, 200, 58, 85, 66, 67, 49, 29, 78, 136, 239, 249, 139, 109, 48, 103, 122, 207, 63, 58, 166, 208, 94, 29, 195, 235, 76, 64, 246, 35, 186, 222, 243, 110, 94, 56, 145, 95, 144, 26, 200, 237, 159, 61, 219, 114, 138, 224, 39, 254, 99, 89, 216, 19, 83, 205, 82])
export const MINTER_KEYPAIR = Keypair.fromSecretKey(MINTER_SECRET_KEY)
export const MINTER_PUBKEY = MINTER_KEYPAIR.publicKey

interface WalletBalancesState {
    balances: WalletBalances | null
    isLoading: boolean
    error: Error | null
    /** The wallet address the current balances belong to. */
    address: string | null
}

interface WalletBalancesActions {
    fetch: (address: string) => Promise<void>
    clear: () => void
}

async function fetchWalletBalances(address: string): Promise<WalletBalances> {
    const owner = new PublicKey(address)

    const [lamports, splAccounts, spl2022Accounts] = await Promise.all([
        connection.getBalance(owner, 'confirmed'),
        connection.getParsedTokenAccountsByOwner(owner, { programId: TOKEN_PROGRAM_ID }, 'confirmed'),
        connection.getParsedTokenAccountsByOwner(owner, { programId: TOKEN_2022_PROGRAM_ID }, 'confirmed'),
    ])

    function parseAccounts(
        accounts: Awaited<ReturnType<typeof connection.getParsedTokenAccountsByOwner>>,
        program: TokenBalance['program'],
    ): TokenBalance[] {
        return accounts.value
            .filter((a) => {
                const info = a.account.data.parsed?.info
                return info && BigInt(info.tokenAmount.amount) > 0n
            })
            .map((a) => {
                const info = a.account.data.parsed.info
                const tokenAmount = info.tokenAmount
                return {
                    mint: new PublicKey(info.mint as string),
                    ata: a.pubkey,
                    amount: BigInt(tokenAmount.amount as string),
                    decimals: tokenAmount.decimals as number,
                    uiAmount: (tokenAmount.uiAmount as number | null) ?? null,
                    program,
                }
            })
    }

    return {
        lamports: BigInt(lamports),
        tokens: [
            ...parseAccounts(splAccounts, 'spl'),
            ...parseAccounts(spl2022Accounts, 'spl2022'),
        ],
    }
}

export const useWalletBalancesStore = create<WalletBalancesState & WalletBalancesActions>()(
    (set, get) => ({
        balances: null,
        isLoading: false,
        error: null,
        address: null,

        fetch: async (address: string) => {
            // Skip if already loading for the same address
            if (get().isLoading && get().address === address) return

            set({ isLoading: true, error: null, address })
            try {
                const balances = await fetchWalletBalances(address)
                console.log(`Fetched balances for ${address}:`, balances)
                // Only apply if address hasn't changed during the fetch
                if (get().address === address) {
                    set({ balances, isLoading: false })
                }
            } catch (err) {
                if (get().address === address) {
                    set({ error: err instanceof Error ? err : new Error(String(err)), isLoading: false })
                }
            }
        },

        clear: () => set({ balances: null, isLoading: false, error: null, address: null }),
    }),
)

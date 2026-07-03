/**
 * Browser stub for `jito-ts`.
 *
 * `@pythnetwork/solana-utils` re-exports a `sendTransactionsJito` helper that
 * imports `jito-ts`, which in turn drags in an ancient `@solana/web3.js@1.77.4`
 * (and an incompatible `rpc-websockets` subpath) that breaks the browser build.
 * We only use the Pyth receiver's regular (non-Jito) transaction path, so we
 * alias the entire `jito-ts` package to these no-op shims. Calling the Jito path
 * would throw — which is correct, since it isn't wired up in the browser.
 */
export class Bundle {
    addTransactions() {
        return this
    }
}

export class SearcherClient {}

export const searcherClient = () => {
    throw new Error("jito-ts is stubbed out in the browser build")
}

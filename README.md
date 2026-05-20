# jbl

A Solana lending protocol with an Anchor program and a React/Vite frontend.

## Repository layout

```
programs/jbl/          Anchor smart contract
crates/jbl-math/       Pure-Rust math library (no external dependencies)
crates/jbl-math-wasm/  Wasm entrypoint — re-exports jbl-math via wasm-bindgen
app/                   React + TypeScript + Vite frontend
tests/                 Anchor integration tests (TypeScript / LiteSVM)
```

---

## jbl-math — Rust crate

`crates/jbl-math` contains the shared interest and share-conversion math used by both the on-chain program and the browser frontend.

### Using from Rust

The crate is included as a path dependency in `programs/jbl/Cargo.toml` with no additional features required.

```toml
[dependencies]
jbl-math = { path = "../../crates/jbl-math" }
```

```rust
use jbl_math::{compute_interest, amount_to_shares, shares_to_amount, amount_to_shares_burned};
```

### Using from the frontend (WebAssembly)

The crate exposes its public functions to JavaScript via [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) when built with the `wasm` feature.

#### Prerequisites

Install [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```sh
cargo install wasm-pack
```

#### Build

Run from the `app/` directory:

```sh
cd app
npm run wasm:build
```

This is equivalent to:

```sh
wasm-pack build ../crates/jbl-math-wasm \
  --target bundler \
  --out-dir ../app/pkg/jbl-math
```

The generated package is written to `app/pkg/jbl-math/` and is gitignored — rebuild whenever the crate changes.

#### Import in TypeScript

```ts
import { sharesToAmount, computeInterest, amountToShares, amountToSharesBurned } from './lib/jblMath'

// The bundler target auto-initializes the .wasm binary on import — no init() call needed.
// All u64 arguments and return values use JavaScript BigInt.
// Option<u64> return types map to bigint | undefined (undefined on overflow).
const interest = computeInterest(1_000_000n, 500, 31_557_600n)
```

#### Vite integration

`vite-plugin-wasm` is already configured in `app/vite.config.ts` — no additional setup is needed.

---

## Frontend (app/)

```sh
cd app
npm install
npm run wasm:build   # compile crates/jbl-math-wasm → app/pkg/jbl-math (required before dev/build)
npm run dev
```

---

## Anchor program

```sh
anchor build
anchor test
```

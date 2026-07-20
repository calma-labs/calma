# calma

A Solana lending protocol with an Anchor program and a React/Vite frontend.

## Setup

```mermaid
flowchart LR
    I("bun install") -->
    A("bun run setup\n─────────────\nanchor build\n+ bun run wasm")

    A --> B("bun run dev\n─────────────\nstart Vite\ndev server")

    A --> C("bun run test\n─────────────\nrun Anchor\nintegration tests")

    A --> D("bun run build\n─────────────\nwasm → app/dist\nproduction bundle")

    D --> E("bun run preview\n─────────────\npreview production\nbuild locally")
```

> **Prerequisites:** [Rust](https://rustup.rs), [Anchor CLI](https://www.anchor-lang.com/docs/installation), [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/), [Bun](https://bun.sh)

## Repository layout

```
programs/calma/     Anchor smart contract (main lending program)
programs/feed/      Price feed program
programs/guard/     Authorization gate
programs/irm/       Interest rate model
crates/math/        Pure-Rust math library (no external dependencies)
crates/state/       Shared account layouts
crates/bindings/    Wasm entrypoint — re-exports math + state via wasm-bindgen
packages/wasm-lib/  Generated @calma/wasm-lib npm package (output of `bun run wasm`)
packages/test/      Anchor integration tests (TypeScript / LiteSVM)
app/                React + TypeScript + Vite frontend
```

---

## math — Rust crate

`crates/math` contains the shared interest and share-conversion math used by both the on-chain program and the browser frontend.

### Using from Rust

Included as a path dependency in `programs/calma/Cargo.toml`:

```toml
[dependencies]
math = { path = "../../crates/math" }
```

```rust
use math::{compute_interest, amount_to_shares, shares_to_amount, amount_to_shares_burned};
```

### Using from the frontend (WebAssembly)

`crates/bindings` exposes math + state to JavaScript via [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) and is packaged as `@calma/wasm-lib`.

#### Prerequisites

Install [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/):

```sh
cargo install wasm-pack
```

#### Build

From the repo root:

```sh
bun run wasm
```

This is equivalent to:

```sh
cargo build --target wasm32-unknown-unknown
wasm-pack build crates/bindings --target bundler --out-dir "$PWD/packages/wasm-lib"
```

The generated package is written to `packages/wasm-lib/` and consumed as `@calma/wasm-lib` — rebuild whenever the crate changes.

#### Import in TypeScript

```ts
import { sharesToAmount, computeInterest, amountToShares, amountToSharesBurned } from '@calma/wasm-lib'

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
bun install
bun run wasm   # compile crates/bindings → packages/wasm-lib (required before dev/build)
bun run dev
```

---

## Anchor program

```sh
anchor build
anchor test
```

## Scoped rules
Path-scoped rules live in `.claude/rules/`. They load automatically when you read matching files — but when CREATING new files in these areas, read the relevant rule first:
- `.claude/rules/frontend-styling.md` — Tailwind CSS v4 / design-token policy (`app/**/*.{tsx,ts,css}`)
- `.claude/rules/wasm-bindings.md` — Wasm client math must replay on-chain `Core` ops (`crates/bindings/**/*.rs`)
- `.claude/rules/token-amounts.md` — Token-amount scaling goes through wasm, never JS float math (`app/**/*.{ts,tsx}`)
- `.claude/rules/testing.md` — test structure, numeric precision, account sets, overflow protocol (`packages/test/**/*.ts`, `programs/*/tests/**/*.rs`, `crates/*/src/lib.rs`)

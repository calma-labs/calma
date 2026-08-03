## Known issues
`docs/known-issues.md` records accepted risks and unresolved defects in the
on-chain programs. Read it before changing anything in `programs/` or
`crates/`, and before reasoning about what the protocol guarantees — several
entries are cases where a code comment states an invariant that no longer
holds. Entries marked **OPEN** are unfixed defects, not accepted trade-offs.
When a change fixes or creates one, update that file in the same commit;
entries are numbered and never renumbered.

## Scoped rules
Path-scoped rules live in `.claude/rules/`. They load automatically when you read matching files — but when CREATING new files in these areas, read the relevant rule first:
- `.claude/rules/frontend-styling.md` — Tailwind CSS v4 / design-token policy (`app/**/*.{tsx,ts,css}`)
- `.claude/rules/wasm-bindings.md` — Wasm client math must replay on-chain `Core` ops; constants/defaults defined in Rust, never transcribed into TS (`crates/bindings/**/*.rs`)
- `.claude/rules/token-amounts.md` — Token-amount scaling goes through wasm, never JS float math (`app/**/*.{ts,tsx}`)
- `.claude/rules/testing.md` — test structure, numeric precision, account sets, overflow protocol (`packages/test/**/*.ts`, `programs/*/tests/**/*.rs`, `crates/*/src/lib.rs`)
- `.claude/rules/program-dependencies.md` — a program links no other program; CPI by hand-rolled discriminator, pin program ids on the account (`programs/*/Cargo.toml`, `crates/*/Cargo.toml`, `Cargo.toml`)

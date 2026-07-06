## Scoped rules
Path-scoped rules live in `.claude/rules/`. They load automatically when you read matching files — but when CREATING new files in these areas, read the relevant rule first:
- `.claude/rules/frontend-styling.md` — Tailwind CSS v4 / design-token policy (`app/**/*.{tsx,ts,css}`)
- `.claude/rules/wasm-bindings.md` — Wasm client math must replay on-chain `Core` ops (`crates/bindings/**/*.rs`)

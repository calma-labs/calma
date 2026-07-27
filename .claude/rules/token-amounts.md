---
paths:
  - "app/**/*.{ts,tsx}"
---

## Token amounts: scale in wasm, never in JS float math

Raw on-chain token amounts are `u64` minor units (a `bigint` in JS). Converting
to/from human-readable decimals is done in the bindings crate and called directly
off `@calma/wasm-lib` — see `.claude/rules/wasm-bindings.md` for the why.

**Never write these in the app:**

- `Number(raw) / 10 ** decimals` — rounds twice and drifts past `2 ** 53`.
- `Math.floor(uiAmount * 10 ** decimals)` / `Math.round(...)` — overflows
  silently at high decimals and builds a **wrong on-chain amount**.

**Use instead** (imported from `@calma/wasm-lib`):

| Need | Call |
| --- | --- |
| user input string → raw amount for a tx | `parse_token_amount(value, decimals)` → `bigint \| undefined`; default `?? 0n` |
| raw → exact string to fill an input (e.g. "Max") | `format_token_amount(raw, decimals)` |
| raw → `number` for display / charts / float math | `token_amount_to_f64(raw, decimals)` |

`parse_token_amount` floors excess digits (matches the chain) and round-trips
with `format_token_amount`. `token_amount_to_f64` is lossy — never feed its result
back into an amount; use `parse_token_amount` for that.

**Plain `bigint` math on raw amounts is fine** and preferred (e.g. a borrow
limit's `min`/subtract, `raw * BigInt(p) / 100n` for a percentage) — it's exact.
Only the human ↔ raw scaling crosses into wasm.

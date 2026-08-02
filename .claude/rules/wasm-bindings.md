---
paths:
  - "crates/bindings/**/*.rs"
---

Wasm bindings consumed by the frontend (`@calma/wasm-lib`). Parses Anchor account
bytes (`state.rs`, `exports.rs`) and exposes client-side protocol math.

## Policy: client math replays on-chain `Core`, never re-derives it

The browser must never compute a protocol quantity (shares, debt, interest, LTV
outcomes) with its own formula. Every such value is produced by replaying the
exact `math::Core` operation the on-chain program runs. There is one
implementation of the math — `crates/math` — and both the program and the client
drive it identically. The reference is the program's instruction handlers in
`programs/calma/src/instructions/` (e.g. `borrow.rs`, `repay.rs`).

1. **No formula duplication here.** Build
   `Core::new(market).with_*(…).accrue_interest()` and call the same operation
   the program calls (`borrow`, `repay`, …) with no-op transfer closures, then
   return the value `Core` itself yields. If you're writing
   `amount * shares / total` in this crate, stop — there's a `Core` op for it.
2. **No new `Core` methods for the client's sake.** That creates a second surface
   that can drift from the operation it summarizes. Reuse the *operations*, not
   bespoke getters. `crates/math/src/core.rs` stays program-shaped.
3. **Real inputs only — no mocks/placeholders.** price ← feed account
   (`interface::PriceFeedHeader`, via `impl math::Oracle for FeedAccount`);
   borrow rate ←
   `irm_state.model.get_fee_bps(utilization)` at the pool's real utilization
   (`IrmRateView` wrapping the real `irm_state::IrmState`); `current_ts` ← caller
   wall clock (mirrors `Clock::get()`). No zero rates, identity oracles, or
   synthetic positions.
4. **Adapters attach to the real types**, matching the program's
   `hooks::irm::IrmState` / `hooks::oracle::read_feed`. The price ratio itself is
   **not** implemented here at all — `impl math::Oracle for FeedAccount`
   delegates to `interface::PriceFeedHeader`, the same code the program runs.
5. **Operations are methods on the view, taking real accounts.** They live on
   `PoolWithIrm` (`state.rs`) and take `&UserPositionAccount` where a position is
   involved. The frontend supplies views, not loose totals.
6. **Faithful failure modes.** Replaying the real op inherits the real gates —
   e.g. `borrow_shares` returns `None` exactly when the on-chain LTV check would
   reject.
7. **No thin wrapper layers in the frontend.** The app calls the wasm view
   methods directly off `@calma/wasm-lib`; pass-through TS modules are not added.

**Why.** The frontend must stay as close to the backend as possible. Every place
that re-expresses protocol math can silently diverge from consensus; replaying
`Core` collapses those to zero.

## Constants and defaults are defined in Rust, never transcribed

Any number the chain also knows — an account size, a fixed-point scale, a byte
offset into an account, a default written into an instruction argument — is
defined in Rust and read from there by the browser. Never write the literal into
TypeScript, however obvious or stable it looks.

wasm-bindgen cannot export a `const`, so each is a zero-argument function in
`lib.rs`:

```rust
#[wasm_bindgen]
pub fn default_price_ttl_ms() -> u32 { interface::DEFAULT_PRICE_TTL_MS }
```

```ts
import { default_price_ttl_ms } from '@calma/wasm-lib'
export const DEFAULT_PRICE_TTL_MS = default_price_ttl_ms()
```

Calling these at module scope is safe: `bindings.js` runs `__wbindgen_start()` in
its own module body, which ES module ordering guarantees completes before any
importer's body.

Put the constant in the crate that owns the concept — `interface` for anything
the provider contract defines, `feed-state` for the reference feed's own
defaults, `math` for protocol arithmetic — and re-export from the app's existing
config module so call sites keep importing from one place.

**Why this is not pedantry.** `POOL_SPACE` was copied into the app with a comment
saying it must stay in sync with the IDL. It drifted 248 bytes low, which
under-allocates the pool account and makes `create` fail — and nothing caught it,
because a comment is not a mechanism. The `Feed` memcmp offsets carried a
hand-drawn layout table that still described the pre-`PriceFeedHeader` struct;
the numbers survived the restructure by luck, which is precisely why nobody
noticed the table had stopped being true.

Where a constant describes a layout rather than a value, add a test that derives
it from the real thing — see `feed_memcmp_offsets_match_the_serialized_layout`,
which serializes a `Feed` and looks for the mints at the exported offsets.

## Token-amount conversions live here too

Base-10 scaling between a UI decimal string and raw `u64` minor units also lives
in this crate (`parse_token_amount` / `format_token_amount` / `token_amount_to_f64`
in `state.rs`), for the same reason — the browser never re-expresses the on-chain
integer/decimal convention in its own JS float math. The app-facing usage rules
are in `.claude/rules/token-amounts.md`.

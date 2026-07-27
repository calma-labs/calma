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
   (`impl math::Oracle for FeedAccount`); borrow rate ←
   `irm_state.model.get_fee_bps(utilization)` at the pool's real utilization
   (`IrmRateView` wrapping the real `irm_state::IrmState`); `current_ts` ← caller
   wall clock (mirrors `Clock::get()`). No zero rates, identity oracles, or
   synthetic positions.
4. **Adapters attach to the real types**, matching the program's
   `hooks::irm::IrmState` / `hooks::oracle::OracleState`.
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

## Token-amount conversions live here too

Base-10 scaling between a UI decimal string and raw `u64` minor units also lives
in this crate (`parse_token_amount` / `format_token_amount` / `token_amount_to_f64`
in `state.rs`), for the same reason — the browser never re-expresses the on-chain
integer/decimal convention in its own JS float math. The app-facing usage rules
are in `.claude/rules/token-amounts.md`.

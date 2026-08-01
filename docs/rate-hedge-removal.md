# Removal of the rate-hedge subsystem

**Date:** 2026-08-01
**Reason:** pre-launch security audit found the settlement accounting unsound in a
way that mints bad debt against lenders on every settlement, and that is cheaply
exploitable by a single party acting as both sides of a hedge.
**Status:** removed from the program, the shared crates, the wasm bindings and the
test suite. Not deprecated, not feature-gated — deleted, so it cannot ship by
accident.

---

## Why it was removed

### 1. Settlement did not conserve value (critical)

`Core::settle_hedge` got the accounting wrong in three places at once:

1. It re-borrowed `borrow_amount` (the principal) rather than
   `borrow_amount + upfront_fee`, so the borrower's ending debt equalled exactly
   what they had received. **The upfront fixed fee they were charged as debt at
   `borrow_with_hedge` was erased at settlement and never collected.**
2. It paid that fee to the provider out of the lend vault and debited
   `total_supply_assets` to match — so lenders funded the provider's fee.
3. The variable-rate excess was collected as **collateral tokens into the pool's
   collateral vault**, an account with no aggregate accounting field and no
   instruction that can ever release those tokens. The lend side — the side that
   was actually short — never received anything.

Net effect per settlement, with the invariant
`vault_balance + total_borrow_assets == total_supply_assets`:

```
pool: 1_000_000 supplied; hedged borrow of 100_000 @ 10_000 upfront fee;
one year at 100% APR; then settle:

excess_lend=110000  excess_collateral=110000  fee_paid=10000
borrower_debt_shares=100000     ← owes exactly the principal; paid nothing
supply_a=1100000  borrow_a=100000  vault=890000
solvency = -110000              ← lend side short by exactly excess_lend
```

The lend side ends short by exactly `excess_lend`, and an equal value of
collateral tokens is stranded in a vault nothing can drain. The subsystem's own
test (`settle_hedge_rolls_debt_and_pays_excess_and_fee`) asserted this same
behaviour — it wrote off 600k of receivable and checked only that
`total_supply_assets` had dropped by the fee. The solvency invariant was never
asserted anywhere, so the hole was invisible to the suite.

### 2. Self-matched hedges gave free borrowing at lenders' expense (critical)

Nothing required the hedge provider and the borrower to be different parties, and
nothing related a provider's posted collateral to the exposure they underwrote.
`math::hedge_excess_collateral` clamped the provider's liability to whatever they
had actually posted and reported the remainder as a `collateral_shortfall` log
line rather than refusing the match.

Combined with (1), the exploit was:

1. Create an offer with `fixed_rate_bps = 1`, `min_duration = 1`,
   `collateral_amount = 1`. Total cost: one base unit plus rent.
2. `borrow_with_hedge(amount = A, duration = 1)`. The upfront fee ceilings to 1
   base unit.
3. One second later, `settle_rate_hedge_match`. The borrower's debt resets to `A`;
   all interest accrued in the window is written off; the provider's liability is
   capped at the 1 unit posted.
4. Repeat — `release_match` restores the offer's capacity, so one offer serves
   indefinitely.

Result: borrow at 0% forever, with the entire interest bill charged to lenders as
uncollectible bad debt.

### 3. Secondary defects in the same subsystem

- `fixed_rate_bps` was stored as `u64`, validated only `> 0`, and narrowed with
  `as u32` at `borrow_with_hedge` — a rate of `2^32 + 500` displayed as
  astronomic and charged 500 bps.
- `duration` was bounded only by the provider's own `max_duration`.
- `Core::settle_hedge` deliberately carried no oracle/LTV gate, so a settlement
  could leave a position above max LTV. That was a defensible choice *given* a
  liquidation path; there is no liquidation path yet.
- `total_supply_assets` was reduced with `saturating_sub`, so settlement could
  silently drive supply assets to zero while supply shares remained outstanding —
  which puts `deposit_lent` into its 1:1 seeding branch and dilutes the next
  depositor to nothing.

---

## What was deleted

### Program — `programs/calma`

Four instructions removed from the `#[program]` module in `src/lib.rs` and their
handler files deleted:

| Instruction | File |
|---|---|
| `create_rate_hedge_offer` | `src/instructions/create_rate_hedge_offer.rs` |
| `borrow_with_hedge` | `src/instructions/borrow_with_hedge.rs` |
| `settle_rate_hedge_match` | `src/instructions/settle_rate_hedge_match.rs` |
| `cancel_rate_hedge_offer` | `src/instructions/cancel_rate_hedge_offer.rs` |

`src/instructions.rs` updated accordingly. The program now exposes 12
instructions (was 16).

Two PDA families no longer exist on-chain:

- `["rate_hedge_offer", pool, authority, fixed_rate_le, min_duration_le, max_duration_le]`
- `["rate_hedge_offer_vault", rate_hedge_offer]` (a token account)
- `["rate_hedge_match", user_position]`

### Shared state — `crates/state`

- Deleted `src/state/hedge_offer.rs`, which defined `RateHedgeOffer` (152 B) and
  `RateHedgeMatch` (112 B) along with `is_cancellable`, `reserve_match`,
  `release_match`, `debit_collateral` and `is_matured`.
- `src/state/mod.rs` updated.
- **`ErrorCode` variants were deliberately kept.** Anchor assigns error numbers by
  declaration order, so deleting a variant silently renumbers every variant after
  it and breaks any client matching on a code. `InvalidDurationRange`,
  `HedgeNotYetMatured` and `OfferHasActiveMatch` remain, marked `RESERVED`.

### Math — `crates/math`

Removed from `src/core.rs`:

- `Core::settle_hedge` and its return type `SettleHedgeOutcome`
- `Core::borrow_with_fee` (only ever called by `borrow_with_hedge`)
- `hedge_total_debt`

Removed from `src/lib.rs`:

- `hedge_excess_collateral`

**Kept:** `lend_to_collateral_amount`. It is not hedge-specific — it is the
inverse of `collateral_to_lend_amount`, is correct and tested, and is the
conversion a liquidation path will need.

Six tests removed from `src/core_tests.rs`
(`settle_hedge_*` ×4, `borrow_with_fee_gates_ltv_on_total_debt`,
`hedge_excess_converts_through_the_oracle_and_clamps`), plus the now-unused
`accrued_core` helper. The oracle-carrying `accrued_core_with_oracle` helper is
still used by the borrow tests and was retained.

### Wasm bindings — `crates/bindings`

- `RateHedgeOfferAccount` / `RateHedgeMatchAccount` wrappers and their
  `from_bytes` impls removed from `src/state.rs`.
- `wasm_parse_rate_hedge_offer` / `wasm_parse_rate_hedge_match` exports removed
  from `src/exports.rs`. **These were part of the wasm ABI** — any consumer
  calling them will fail to resolve the symbol after the next `npm run wasm`.
  Nothing in `app/` or `packages/` referenced them.
- Layout pins for the two structs removed from the `struct_sizes` test.
- Two roundtrip tests removed from `tests/state_roundtrip.rs`.

### Tests and docs

- Deleted `packages/test/rate-hedge.ts` (~900 lines). It was picked up by the
  `./packages/test/*.ts` glob in the root `test` script.
- `ARCHITECTURE.md`: hedge instructions removed from the instruction graph, the
  two entities and their relationships removed from the ER diagram, intro and
  crate-summary boxes updated.

---

## Verification

```
cargo check --workspace --all-targets     # clean: no errors, no dead-code warnings
cargo test  --workspace                   # 204 passed, 0 failed
anchor build                              # all five programs build; IDLs regenerated
```

Regenerated `target/idl/calma.json` after the removal:

- instructions: `borrow`, `claim_fees`, `create`, `deposit_collateral`,
  `deposit_lent`, `flash_borrow`, `flash_repay`, `process_queue_entry`, `repay`,
  `set_fee`, `withdraw_collateral`, `withdraw_lent` — 12, none hedge-related
- accounts: `Pool`, `UserPosition`
- the string `hedge` does not appear anywhere in the IDL

The only remaining `hedge` references in the source tree are the three RESERVED
`ErrorCode` variants described above.

---

## Migration / deployment notes

- **This is a breaking IDL change.** Re-run `anchor build` to regenerate
  `target/idl/calma.json` and `target/types/calma.ts`, and `npm run wasm` to
  regenerate `@calma/wasm-lib`.
- If a version of this program with the hedge instructions was ever deployed to a
  cluster with live accounts, those `RateHedgeOffer` / `RateHedgeMatch` accounts
  and their collateral vaults become **permanently unreachable** — no instruction
  can close them or return the locked collateral. Confirm none exist before
  upgrading, or ship a one-shot drain instruction first. On a fresh deployment
  this does not apply.
- Error-code numbering is unchanged, so existing client error handling keeps
  working.

---

## If this is rebuilt later

The economics need to be specified before the code is. Settlement must conserve
value; the invariant to hold and to assert in tests is:

```
vault_balance + total_borrow_assets == total_supply_assets + assets_in_queue
```

Specifically, a correct design needs at minimum:

1. The borrower's ending debt to be `principal + upfront_fee` — the fixed leg has
   to actually be charged.
2. The provider's excess payment to reach the **lend** side, credited to
   `total_supply_assets`, not deposited as collateral tokens into a vault with no
   accounting.
3. Interest not to accrue on the phantom fee debt, or the excess formula to net it
   out — as written the provider paid the full floating interest rather than the
   spread over the fixed rate, overpaying by roughly `2 × upfront_fee`.
4. Provider collateral checked against the exposure being underwritten at match
   time, with the match refused if it does not cover the plausible worst case.
5. Self-matching disallowed, or made economically pointless by (1)–(4).
6. `fixed_rate_bps` stored as `u32` and validated against `math::MAX_RATE_BPS`;
   `duration` bounded by a protocol-level maximum, not only the provider's.
7. A liquidation path in place first — settlement is deliberately un-gated on LTV,
   which is only safe if an over-LTV position can subsequently be closed.

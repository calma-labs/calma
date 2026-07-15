---
paths:
  - "packages/test/**/*.ts"
  - "programs/*/tests/**/*.rs"
  - "crates/*/src/lib.rs"
---

# Test writing rules

## Where tests live

| Layer | Location | Runner |
|-------|----------|--------|
| Pure math unit tests | `#[cfg(test)] mod tests` inside `crates/math/src/lib.rs` | `cargo test -p math --lib` |
| Rust program integration | `programs/jbl/tests/*.rs` (LiteSVM) | `cargo test -p jbl` |
| TypeScript end-to-end | `packages/test/*.ts` (one file per feature) | `anchor test` |

Never use `npx ts-mocha` or `npx mocha` directly — TypeScript tests need Anchor's validator context (`ANCHOR_PROVIDER_URL`, workspace IDLs, deployed programs). Always use `anchor test`.

---

## Rust unit tests (`crates/math/src/lib.rs`)

### Coverage protocol: test four value bands for every function

Every public math function must have test cases in all four bands:

1. **Zero / degenerate** — zero principal, zero shares, zero elapsed time → `Some(0)` or early return
2. **Minimal (1 base unit)** — verify ceiling division rounds up, integer truncation of LTV, etc.
3. **Large but valid (≥ 10^13 base units)** — 100M tokens with 6 decimals = `10^14`. Use the shared constant:
   ```rust
   const HUNDRED_M: u64 = 100_000_000 * 1_000_000; // 10^14 base units
   ```
4. **Near / over u64::MAX** — must-succeed boundary and must-return-`None` overflow

### Overflow protocol

- Functions returning `Option<u64>` must have a test that returns `None`. Never write a test whose only claim is "doesn't panic" with no assertion — that's `graceful_none_on_u64_overflow` (already exists as a legacy smell; don't repeat it).
- Finding the exact None boundary: work out the algebra. For `compute_interest`, `u64::MAX` at 100% APR for `YEAR` seconds cancels exactly to `Some(u64::MAX)`; adding one second flips it to `None`. Document the calculation in the test name or a short comment.
- `max_borrowable` uses `saturating_mul` — it does **not** return `Option`. At `u64::MAX` it clamps to `u64::MAX / 100`, not None. Test this explicitly.

### Round-trip tolerance

`shares_to_amount` uses **ceiling** division; `amount_to_shares` uses **floor** division. A round-trip `amount → shares → amount` may differ by ±1 unit. Assert:
```rust
assert!(back.abs_diff(original) <= 1);
```
Never assert `== original` on a round-trip involving both directions.

### Constants available in `mod tests`

```rust
const YEAR: u64 = SECONDS_PER_YEAR; // 31_557_600
// PRICE_SCALE and flash_fee are accessible via `use super::*`
```

---

## TypeScript end-to-end tests (`packages/test/*.ts`)

### File structure

One `describe` block per scenario, each with its own `before(async () => { ... })` that calls `setupTest()`. Never share pool state between top-level `describe` blocks — each gets a fresh pool. Keep helpers (`depositCollateral`, `borrow`, `repay`, `depositLent`) local to the file; don't import them across test files.

### `setupTest()` defaults

- Mints **1,000,000,000 base units** (1B) of each token to `authority` — this is the per-user ceiling unless you call `mintTo` again.
- Oracle price: **1:1** (`collateralPrice = lendPrice = 1_000_000`).
- LTV: **75%**, `max_feed_age_secs = 90`.
- IRM: linear 0–500 bps at 0–100% utilization.

For amounts above 1B base units, use `mintTo` before depositing:
```typescript
await mintTo(setup.connection, setup.payer, setup.collateralMint,
    setup.userCollateralTokenAccount, setup.authority, BigInt(amount.toString()));
```

### Numeric precision

- Amounts ≤ 900_000_000 (well under `Number.MAX_SAFE_INTEGER`) can be plain `number`.
- Amounts > 900_000_000 **must** use `new BN("...")` with a string literal — never `new BN(some_js_number)` for values that can exceed 2^53.
- `participateInPool(setup, amount)` takes a plain `number`. For large lend deposits, call `depositLent` directly:
  ```typescript
  async function depositLent(setup: TestSetup, amount: BN) {
      await setup.program.methods.depositLent(amount)
          .accounts({ pool: setup.pool, lendMint: setup.lendMint,
                      authority: setup.authority.publicKey,
                      userLendTokenAccount: setup.userLendTokenAccount })
          .signers([setup.authority]).rpc();
  }
  ```

### Instruction account sets (copy exactly — wrong accounts cause cryptic errors)

**`borrow`** — includes oracle accounts:
```typescript
{ pool, lendMint, authority: authority.publicKey,
  rateProgram: irmProgramId, irmState: irmConfig,
  feedProgram: feedProgram.programId, feedState: feedPda }
```

**`repay`** — does NOT read the oracle; omit feed accounts:
```typescript
{ pool, lendMint, authority: authority.publicKey,
  rateProgram: irmProgramId, irmState: irmConfig }
```

**`depositCollateral`**:
```typescript
{ pool, collateralMint, authority: authority.publicKey,
  userTokenAccount: userCollateralTokenAccount }
```

**`depositLent` / `participate`**:
```typescript
{ pool, lendMint, authority: authority.publicKey,
  userLendTokenAccount }
```

### Vault-balance check fires before LTV check

`borrow_handler` validates `lend_vault.amount >= amount` before calling `Core::borrow()`. If the vault is empty, the error is **`InsufficientFunds`**, not `Undercollateralized`.

Consequence: when testing that an over-LTV borrow is rejected, the lend vault must still contain at least the borrow amount. Deposit a buffer beyond the maximum-capacity amount:

```typescript
// Bad: vault is exactly drained by the max borrow → InsufficientFunds, not Undercollateralized
await depositLent(setup, MAX_CAPACITY);
await borrow(setup, MAX_CAPACITY);
await borrow(setup, 1); // ❌ fails with InsufficientFunds, not the expected LTV error

// Good: 5M buffer keeps vault non-empty so LTV guard fires
await depositLent(setup, MAX_CAPACITY + 5_000_000);
await borrow(setup, MAX_CAPACITY);
await borrow(setup, 1); // ✅ Undercollateralized
```

### Error assertion helper

Catching `expect.fail()` inside the same `catch` block causes a false negative (the `fail` message doesn't include "Undercollateralized" so the outer check re-fails). Guard against it:

```typescript
async function expectRejected(label: string, fn: () => Promise<void>) {
    try {
        await fn();
        expect.fail(`expected ${label} to be rejected`);
    } catch (e: any) {
        const msg = e.message as string;
        if (msg.includes("expected") && msg.includes("to be rejected")) throw e;
        expect(msg).to.include("Undercollateralized");
    }
}
```

### Oracle price math

`oracle_price = collateralPrice × PRICE_SCALE / lendPrice` (integer division; equal token decimals cancel).

Borrow capacity = `collateral × oracle_price / PRICE_SCALE × ltv_percent / 100`.

Key boundary: if `collateralPrice < lendPrice / PRICE_SCALE`, integer division floors `oracle_price` to **0** → capacity is 0 → every borrow is rejected. Test this edge explicitly when relevant.

When `setFeedPrice` is called mid-test, the next `borrow` immediately reflects the new price — there is no cache delay in LiteSVM.

### `repay` safely accepts overpayment

`Core::repay` caps at `amount.min(total_due)`. Pass `u64::MAX` or a 2× multiple to fully clear a position without knowing the exact accrued interest. Safe to use in teardown steps.

### Numeric assertions on `BN` fields

Anchor account fields come back as `BN`. Never use `.toNumber()` — it silently truncates values above 2^53.

- **Equality**: use `.toString()` and compare strings
  ```typescript
  expect(pool.market.totalBorrowAssets.toString()).to.equal("75000000000000");
  ```
- **Inequality / ordering**: use BN's comparators (`.gte()`, `.lte()`, `.gt()`, `.lt()`)
  ```typescript
  expect(pool.market.totalBorrowAssets.gte(new BN(INITIAL_BORROW))).to.be.true;
  ```

---

## Verification

```bash
# Unit tests only (fast, no validator)
cargo test -p math --lib

# Full suite (builds programs, starts localnet, runs all packages/test/*.ts)
anchor test
```

The surfpool test (`packages/test/surfpool.ts`) requires a live mainnet Pyth feed and will fail in offline environments — this is expected and pre-existing.

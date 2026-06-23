# One Source of Truth for Backend and Frontend

### Compiling the same logic into your server *and* your browser

*A pattern talk — stack-agnostic, examples drawn from a real codebase*

---

## A Bug You've Probably Shipped

A user opens the app. The form previews the outcome of their action:

> *"You'll receive ≈ **4.7%**."*

They click. The server runs, commits, and charges them **5.1%**.

Nobody wrote a bug, exactly. The preview was computed in TypeScript on the
client. The real number was computed in Rust on the server. Months ago the two
were identical. Then someone fixed a rounding edge case on the server — and the
TypeScript copy was never touched.

The user doesn't care which language was right. They saw one number and got another.

---

## The Disease Behind That Bug

It's not a rounding bug. It's a **structural** one:

> The same rule is implemented **twice**, in two languages, and the two copies
> are kept in sync **by hand.**

```
            ┌─────────────────────┐         ┌─────────────────────┐
            │  Backend            │         │ Frontend (TS)       │
   rule  →  │  fee = f(x, y)      │   ≠     │  fee = f'(x, y)      │  ← reimplemented
            │  layout = struct A  │         │  layout = guessed   │  ← hand-mirrored
            └─────────────────────┘         └─────────────────────┘
```

Anything synced by hand drifts:

- A rule changes on one side; the other is forgotten.
- A persisted record gains a field; the client parser silently reads garbage.
- The preview and the commit slowly disagree. Users notice before you do.

The usual fixes — **shared JSON schema**, **codegen**, **a preview-only API
endpoint** — all keep *two implementations* and bolt a sync step on top. They
reduce drift. They don't remove the thing that drifts.

---

## The Thesis

> Stop trying to *sync two implementations.*
> Have **one** — and compile it twice.

Write the core logic once, in a language that targets **both** your backend's
native platform **and** **WebAssembly**. Ship the WASM build to the frontend as
an ordinary package.

```
                    ┌──────────────────────────┐
                    │   core logic (one crate) │
                    │   math · rules · layout  │
                    └────────────┬─────────────┘
                       compiles to two targets
                  ┌────────────────┴────────────────┐
                  ▼                                  ▼
        ┌───────────────────┐              ┌───────────────────┐
        │  native binary    │              │  WASM module      │
        │  (the backend)    │              │  (the browser)    │
        └───────────────────┘              └───────────────────┘
              authoritative                   identical preview
```

Same source. Same compiler. Two artifacts. The preview and the commit can't
drift because **they're the same code.**

The rest of the talk is how you actually build that trunk — three steps — and
then a bonus the type system throws in for free.

---

## Step 1 — A core that depends on nothing

Pull the rules into a crate that knows nothing about either runtime.

- No web framework. No DB driver. No browser APIs.
- Ideally **zero dependencies** — just the language.

```rust
// crate: core-math   (zero dependencies)
pub fn amount_to_shares(amount: u64, total: u64, shares: u64) -> Option<u64> { … }
pub fn compute_interest(principal: u64, rate_bps: u32, secs: u64) -> Option<u64> { … }
```

Why this is the whole foundation: a crate with no runtime assumptions compiles
**anywhere**, including WASM — where most of your normal dependencies would not
even build. Keep this crate pure and both targets fall out for free.

---

## Step 2 — Ask for *data*, not for the runtime

The core needs inputs — balances, the clock, a price — but it must not know
*where they come from*. So it asks through **traits**, not concrete types:

```rust
pub trait Market   { fn total_supply(&self) -> u64; fn last_update(&self) -> i64; … }
pub trait Position { fn collateral(&self) -> u64;   fn debt_shares(&self) -> u64;  }
pub trait Clock    { fn now(&self) -> i64; }
```

- On the **backend**, these are implemented by your real persisted records and
  the system clock.
- On the **frontend**, the same traits are implemented by data fetched over the
  wire (or by test fixtures).

The logic is written once against the *interface*. Each side plugs in its own
source. The core never learns which world it's running in.

And keep effects out of it — the core decides *what* should happen, the caller
*does* it:

```rust
core.borrow(amount, |amt| transfer_to_user(amt))?;
//                  └────────── caller supplies the effect ──────────┘
```

Backend passes a real transfer. Frontend passes a no-op — it's only previewing.

---

## Step 3 — Share the layout, not just the math

The frontend doesn't only recompute numbers — it has to **decode the records the
backend persisted.** Re-deriving that byte layout by hand is exactly where silent
corruption lives.

So define the record **once**, with a fixed, explicit binary layout, and let both
sides use that one definition:

```rust
#[repr(C)]                 // stable field order
pub struct Pool {
    pub total_supply: u64,
    pub total_borrow: u64,
    pub last_update:  i64,
    _pad: [u8; 7],         // explicit padding — no uninitialized bytes
    _reserved: [u8; 32],   // room to grow without breaking the layout
}
```

The backend stores the raw bytes. The frontend receives them and casts straight
back into the *same struct* — no schema, no JSON, no hand-written parser:

```rust
fn parse<T: Pod>(bytes: &[u8]) -> Option<T> {
    if bytes.len() != size_of::<T>() { return None; }   // length check
    Some(bytemuck::pod_read_unaligned(bytes))           // bytes -> struct
}
```

Two facts make this safe across machines:

1. **One struct definition is the contract.** Change it once; both sides move together.
2. Both targets are **little-endian**, so the bytes need no translation.

Pin it with a test so a layout change can't sneak through unnoticed:

```rust
assert_eq!(size_of::<Pool>(), 49_520);   // breaks loudly if the layout shifts
```

---

## The Payoff — one flow, two runtimes

That's the whole trunk. Here's what it gets you:

```
Frontend (WASM)                          Backend (native)
──────────────                           ────────────────
PoolView.from_bytes(raw)                 load record (same struct)
core::amount_to_shares()  ── same ──►    core::amount_to_shares()
core::health_factor()       code         core::compute_interest()
   live preview, no RPC                  core::borrow(amt, transfer)
                                         persist record back
```

- The preview the user sees and the result the server commits run
  **byte-identical code.**
- The frontend decodes persisted records through the **same struct** the backend
  wrote them with.
- A change to a rule or a field is **one edit**, recompiled into both artifacts.

The 4.7%-vs-5.1% bug from the first slide is now **unrepresentable.** There's no
second implementation left to drift.

---

## Wiring it up: a thin WASM surface

One small adapter crate compiles the core to WASM and exposes a JS-friendly API.
It holds **no logic** — it only forwards into the shared core:

```rust
#[wasm_bindgen]
pub fn amount_to_shares(amount: u64, total: u64, shares: u64) -> Option<u64> {
    core_math::amount_to_shares(amount, total, shares)   // re-export, nothing more
}

#[wasm_bindgen]
pub struct PoolView(Pool);

#[wasm_bindgen]
impl PoolView {
    pub fn from_bytes(b: &[u8]) -> Option<PoolView> { parse(b).map(Self) }
    pub fn utilization_bps(&self) -> u16 { /* calls into core */ }
    pub fn health_factor(&self, …) -> Option<u32> { /* calls into core */ }
}
```

Build it (`wasm-pack` or equivalent) → out comes a normal package the frontend
installs and imports like any other dependency. No special integration.

---

## Bonus — because it's Rust, illegal states won't compile

Everything so far works in any compile-to-WASM language. This part is the
type-system dividend.

The scariest bugs in this kind of domain aren't bad arithmetic — they're
**operations in the wrong order.** Computing a new loan *before* settling accrued
interest, say. A runtime check catches it *if you remember to write it.*

Make it impossible instead. Encode "what has happened" into the type — a
**typestate**:

```rust
Core<…, NotAccrued>          // freshly loaded
  .accrue_interest()   ->  Core<…, Accrued>   // the only way to get an Accrued

// borrow() exists ONLY for the Accrued state:
impl Core<…, Accrued> {
    pub fn borrow(&mut self, amount: u64, transfer: impl Fn(u64)) -> Result<…> { … }
}
```

Call `borrow()` before `accrue_interest()` and **it doesn't compile** — the
`Accrued` token literally doesn't exist yet.

> The single most safety-critical rule in the domain becomes a **compile error**
> — and because the core is shared, it's enforced identically on backend and
> frontend, for free.

---

## What the Pattern Buys You

| Concern | Two implementations | One impl, two targets |
|---|---|---|
| Math drift | Manual sync, silent bugs | Impossible — same function |
| Layout drift | Hand-written parser rots | Same struct, size-asserted |
| Illegal operation order | Runtime check (if remembered) | Compile error, both sides |
| Preview accuracy | "Close enough" | Exact |
| Adding a field / rule | Edit N places | Edit once, rebuild |

Cost: a two-target build (native + WASM) and the discipline to keep the core
dependency-free.

---

## When to Reach for It

**Good fit**

- Non-trivial shared math / validation between server and client
- Binary or size-sensitive persisted records
- Correctness-critical invariants you don't want duplicated
- A team already comfortable with a systems language

**Probably overkill**

- CRUD with trivial client-side logic
- Logic that is genuinely server-only (no preview needed)
- No appetite for a compile-to-WASM toolchain

Be honest about the niche: the payoff scales with how much logic is shared and
how much binary layout matters. For a thin CRUD app, a shared schema is plenty.

---

## Takeaways

1. **Drift is the disease.** Two implementations of one rule *will* diverge — by hand-sync, guaranteed.
2. **Compile, don't copy.** One core crate → native binary **and** WASM.
3. **Traits decouple** the logic from where the data lives; closures keep effects out.
4. **One `#[repr(C)]` struct** makes persistence and parsing the same contract.
5. The preview and the commit become, literally, **the same code** — the opening bug can't exist.
6. **Bonus, courtesy of Rust:** typestates turn ordering invariants into compile errors, on both sides.

---

### Questions?

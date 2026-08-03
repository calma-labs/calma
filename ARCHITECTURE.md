# Architecture

Calma is a decentralized lending protocol on Solana. Users deposit collateral to borrow tokens and lenders earn LP yields.

> The fixed-rate hedging subsystem was removed before launch — see [docs/rate-hedge-removal.md](docs/rate-hedge-removal.md).
>
> This document describes how the system is *put together*. For what it does
> **not** guarantee — accepted risks and open defects, including the absence of a
> liquidation path — see [docs/known-issues.md](docs/known-issues.md). Read that
> before reasoning about what the protocol protects.

---

## Monorepo Structure

```mermaid
graph TD
    subgraph NPM["NPM Workspace"]
        APP["app/\nReact + TypeScript frontend"]
        TEST["packages/test/\nAnchor integration tests"]
        WASM_PKG["packages/wasm-lib/\n@calma/wasm-lib\n(generated)"]
    end

    subgraph CARGO["Cargo Workspace"]
        subgraph PROGS["programs/ — deployed Anchor programs"]
            P_CALMA["calma\nthe lending protocol"]
            P_FEED["feed\nreference oracle"]
            P_IRM["irm\nreference rate model"]
            P_GUARD["guard\nreference whitelist"]
            P_QUOTE["quote\nalternative provider\n(oracle + rate in one)"]
            P_FAUCET["faucet\ntest-only mint / mock swap"]
        end

        subgraph CRATES["crates/ — shared libraries"]
            MATH["math\nprotocol arithmetic"]
            STATE["state\ncalma's accounts"]
            IFACE["interface\noracle ABI"]
            IRMS["irm-state\nirm's accounts"]
            FEEDS["feed-state\nfeed's accounts"]
            WASM_CRATE["bindings\nwasm-bindgen"]
        end
    end

    WASM_CRATE -->|"wasm-pack build"| WASM_PKG
    APP --> WASM_PKG
    TEST --> WASM_PKG
```

> **No program links another program.** `calma` reaches its oracle, rate model
> and whitelist through pubkeys recorded on its own `Pool`, never through a
> Cargo dependency — see [Pluggable providers](#pluggable-providers) and
> `.claude/rules/program-dependencies.md`.

---

## Component Overview

```mermaid
graph LR
    subgraph Frontend["app/ — React Frontend"]
        UI["Pages\nMarket · Pool · Portfolio\nMultiply · Create · Feed"]
        HOOKS["Hooks & Stores\nReact Query · Zustand"]
        LIB["lib/\nprogram.ts · transactions.ts\npoolDisplay.ts"]
    end

    subgraph Chain["Solana Network"]
        ANCHOR["programs/calma"]
        PROVIDERS["Provider programs\nfeed · irm · guard\n(or any replacement)"]
        ACCOUNTS["Accounts\nPool · UserPosition"]
    end

    subgraph Shared["Shared Rust Logic"]
        CALMA_MATH["math\nCore typestate\nInterest · Shares · LTV"]
        CALMA_STATE["state\nPool · UserPosition\nWithdrawalQueue"]
        CALMA_IFACE["interface\nPriceFeedHeader\nread_price_feed()"]
    end

    subgraph WASM["WASM Bridge"]
        CALMA_WASM["bindings\nReplays Core ops\nExports Rust constants"]
        WASM_LIB["@calma/wasm-lib\nGenerated npm package"]
    end

    UI --> HOOKS
    HOOKS --> LIB
    LIB -->|"Anchor IDL + @solana/kit"| Chain
    LIB --> WASM_LIB
    WASM_LIB --> CALMA_WASM
    CALMA_WASM --> CALMA_MATH
    CALMA_WASM --> CALMA_STATE
    CALMA_WASM --> CALMA_IFACE
    ANCHOR --> CALMA_MATH
    ANCHOR --> CALMA_STATE
    ANCHOR --> CALMA_IFACE
    ANCHOR --> ACCOUNTS
    ANCHOR -.->|"CPI / account read\nby pinned pubkey"| PROVIDERS
```

---

## Pluggable providers

A market records four provider pubkeys on its own `Pool` at creation and they are
immutable afterwards. `calma` knows the *shape* of each role, never the identity
of an implementation — which is why `programs/quote` can serve a market as both
oracle and rate model despite `calma` never having linked it.

| Role | Pinned on `Pool` | Mechanism | Contract |
|------|------------------|-----------|----------|
| Oracle | `feed_state` + `feed_program` | **Passive account read** — no CPI | Account begins with `interface::PriceFeedHeader` after its 8-byte discriminator; `read_price_feed` checks address *and* owner |
| Rate model | `irm_state` + `rate_program` | CPI, hand-rolled discriminator | Instructions named `borrow_rate(u64)` and `check_authority(Pubkey)`; accounts `[irm_state, pool]`; rate returned as `u32` LE in return data |
| Whitelist | `guard_state` + `guard_program` | CPI, hand-rolled discriminator | Instruction named `check(Pubkey)`; account `[guard_state]` |

The discriminators are `sha256("global:<name>")[..8]`, hardcoded in
`programs/calma/src/hooks/{irm,guard}.rs` and asserted against Anchor's own
derivation by a unit test in each file. The oracle role has no discriminator to
check on purpose: every implementation names its account struct differently, and
the address is already pinned to one exact account.

```mermaid
graph LR
    POOL["Pool\n(pins 4 program ids)"]
    CALMA["programs/calma"]

    subgraph REF["Reference implementations"]
        FEED["feed"]
        IRM["irm"]
        GUARD["guard"]
    end

    subgraph ALT["A market may choose instead"]
        QUOTE["quote\noracle + rate"]
        ANY["any third-party program\nmatching the contract"]
    end

    CALMA --> POOL
    POOL -.->|"read"| FEED
    POOL -.->|"CPI"| IRM
    POOL -.->|"CPI"| GUARD
    POOL -.->|"read + CPI"| QUOTE
    POOL -.-> ANY
```

---

## Data Flow — Borrow Example

```mermaid
sequenceDiagram
    participant User
    participant React as React Frontend
    participant WASM as @calma/wasm-lib
    participant RPC as Solana RPC
    participant Program as calma (on-chain)
    participant Rate as rate_program
    participant Feed as feed_state account

    User->>React: Enter borrow amount
    React->>WASM: PoolWithIrm.borrow_shares(amount)
    Note over WASM: replays the same math::Core ops<br/>the program will run
    WASM-->>React: estimated debt shares
    React->>RPC: send borrow transaction
    RPC->>Program: borrow_handler(ctx, amount)
    Program->>Feed: read PriceFeedHeader (address + owner pinned)
    Feed-->>Program: price, staleness budget
    Program->>Rate: CPI borrow_rate(utilization)
    Rate-->>Program: rate_bps (return data, attributed to caller)
    Program->>Program: Core::accrue_interest() → Accrued
    Program->>Program: Core::borrow() — LTV gate + share mint
    Program->>Program: transfer lend tokens to user
    Program-->>RPC: transaction confirmed
    RPC-->>React: confirmation
    React-->>User: success toast + updated UI
```

---

## Rust Crate Dependencies

`math` depends on nothing — not even `anchor-lang`. That is what lets the exact
same arithmetic run on-chain and in the browser.

```mermaid
graph BT
    MATH["math\n─────────\nCore&lt;M,I,P,O,S&gt; typestate\ncompute_interest()\namount_to_shares()\nmax_borrow_capacity()\ntraits: Market · Position\nOracle · IrmRate · FeeModel"]
    IFACE["interface\n─────────\nPriceFeedHeader\nread_price_feed()\nprice_stale_at()"]
    STATE["state\n─────────\nPool (49 KB, zero-copy)\nUserPosition (88 B)\nWithdrawalQueue"]
    IRMS["irm-state\n─────────\nIrmState\nPiecewiseLinearModel"]
    FEEDS["feed-state\n─────────\nFeed · FeedRules\nPriceSource"]
    WASM["bindings\n─────────\n#[wasm_bindgen]\nexports"]

    PROG["programs/calma\n─────────\n11 instructions"]
    PFEED["programs/feed"]
    PIRM["programs/irm"]
    PQUOTE["programs/quote"]

    IFACE --> MATH
    STATE --> MATH
    IRMS --> MATH
    FEEDS --> IFACE
    WASM --> MATH
    WASM --> STATE
    WASM --> IFACE
    WASM --> IRMS
    WASM --> FEEDS
    PROG --> MATH
    PROG --> STATE
    PROG --> IFACE
    PFEED --> FEEDS
    PIRM --> IRMS
    PQUOTE --> IFACE
```

`programs/guard` and `programs/faucet` depend on no workspace crate.

---

## On-Chain Program Instructions

`programs/calma` exposes **11** instructions.

```mermaid
graph TD
    subgraph Pool["Pool Management"]
        CREATE["create\nInitialise market,\npin all provider ids"]
        CLAIM["claim_fees\nMint accrued fee shares\n(inert while fee == 0)"]
    end

    subgraph Collateral["Borrow Side"]
        DEP_COL["deposit_collateral"]
        WITH_COL["withdraw_collateral"]
        BORROW["borrow"]
        REPAY["repay"]
    end

    subgraph Lending["Lend Side"]
        DEP_LENT["deposit_lent\nMint LP shares"]
        WITH_LENT["withdraw_lent\nBurn LP shares →\nimmediate or queued"]
        PROC_Q["process_queue_entry\nDrain queue head (FIFO)"]
    end

    subgraph Flash["Flash Loans"]
        FL_BORROW["flash_borrow"]
        FL_REPAY["flash_repay"]
    end

    WITH_LENT -->|"queue non-empty"| PROC_Q
    FL_BORROW -->|"same transaction,\nsysvar-enforced"| FL_REPAY
```

Guarded entry points (`deposit_collateral`, `borrow`, `deposit_lent`) call the
whitelist. Exits (`repay`, `withdraw_collateral`, `withdraw_lent`,
`process_queue_entry`) are deliberately never gated, so a de-whitelisted user can
always leave. `repay` and `deposit_collateral` do not read the oracle at all.

---

## Account State

Supply- and borrow-side accounting lives in a nested `Market` struct rather than
directly on `Pool`; the field names below are the current ones (see
`crates/state/src/state/pool.rs`).

```mermaid
erDiagram
    Pool {
        pubkey authority
        pubkey collateral_mint
        pubkey lend_mint
        pubkey lp_mint
        Market market
        pubkey rate_program
        pubkey irm_state
        pubkey feed_program
        pubkey feed_state
        pubkey guard_program
        pubkey guard_state
        u8 lp_mint_bump
        WithdrawalQueue withdrawal_queue
    }

    Market {
        u64 total_supply_assets
        u64 total_supply_shares
        u64 total_borrow_assets
        u64 total_borrow_shares
        i64 last_update
        u64 fee
        u64 accrued_fee_shares
        u64 assets_in_queue
        u64 flash_loan_outstanding
        u8 ltv_percent
    }

    UserPosition {
        pubkey authority
        pubkey pool
        u64 collateral_deposited
        u64 debt_shares
        u8 bump
    }

    Pool ||--|| Market : "embeds"
    Pool ||--o{ UserPosition : "has many"
```

> Collateral is **not** totalled on `Pool` — it is tracked per `UserPosition`
> only, and the vault balance is the aggregate.

---

## Build Pipeline

```mermaid
flowchart LR
    CRATE_SRC["crates/\nmath · state · interface\nirm-state · feed-state"]
    WASM_SRC["crates/bindings\n(Rust source)"]
    PROG_SRC["programs/\ncalma · feed · irm\nguard · quote · faucet"]
    APP_SRC["app/src\n(TypeScript source)"]

    ANCHOR_BUILD["anchor build\n→ target/deploy/*.so\n→ target/idl/*.json"]
    WASM_BUILD["wasm-pack build\n→ packages/wasm-lib/"]
    VITE_BUILD["vite build\n→ app/dist/"]

    CRATE_SRC --> ANCHOR_BUILD
    PROG_SRC --> ANCHOR_BUILD
    CRATE_SRC --> WASM_BUILD
    WASM_SRC --> WASM_BUILD
    WASM_BUILD --> VITE_BUILD
    ANCHOR_BUILD -->|"IDL + generated types"| VITE_BUILD
    APP_SRC --> VITE_BUILD
```

`npm run setup` runs the whole chain once. `packages/wasm-lib/` is generated
output and is never committed — the app cannot typecheck against a new Rust
export until `npm run wasm` regenerates it.

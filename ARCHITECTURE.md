# Architecture

Calma is a decentralized lending protocol on Solana. Users deposit collateral to borrow tokens and lenders earn LP yields.

> The fixed-rate hedging subsystem was removed before launch — see [docs/rate-hedge-removal.md](docs/rate-hedge-removal.md).

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
        PROGRAM["programs/calma\nAnchor smart contract"]
        MATH["crates/math\nPure Rust math library"]
        STATE["crates/state\nAccount type definitions"]
        WASM_CRATE["crates/bindings\nwasm-bindgen bindings"]
    end

    WASM_CRATE -->|"wasm-pack build"| WASM_PKG
    WASM_CRATE --> MATH
    WASM_CRATE --> STATE
    PROGRAM --> MATH
    PROGRAM --> STATE
    APP --> WASM_PKG
    TEST --> WASM_PKG
```

---

## Component Overview

```mermaid
graph LR
    subgraph Frontend["app/ — React Frontend"]
        UI["Pages\nMarket · Pool · Portfolio\nMultiply · Create"]
        HOOKS["Hooks & Stores\nReact Query · Zustand"]
        LIB["lib/\nprogram.ts · transactions.ts\npoolDisplay.ts"]
    end

    subgraph Chain["Solana Network"]
        ANCHOR["programs/calma\nAnchor Program"]
        ACCOUNTS["Accounts\nPool · UserPosition"]
    end

    subgraph Shared["Shared Rust Logic"]
        CALMA_MATH["math\nInterest · Shares · LTV"]
        CALMA_STATE["state\nPool · UserPosition\nWithdrawalQueue · Fees"]
    end

    subgraph WASM["WASM Bridge"]
        CALMA_WASM["bindings\nExposes math + state\nto JavaScript"]
        WASM_LIB["@calma/wasm-lib\nGenerated npm package"]
    end

    UI --> HOOKS
    HOOKS --> LIB
    LIB -->|"@solana/kit"| Chain
    LIB --> WASM_LIB
    WASM_LIB --> CALMA_WASM
    CALMA_WASM --> CALMA_MATH
    CALMA_WASM --> CALMA_STATE
    ANCHOR --> CALMA_MATH
    ANCHOR --> CALMA_STATE
    ANCHOR --> ACCOUNTS
```

---

## Data Flow — Borrow Example

```mermaid
sequenceDiagram
    participant User
    participant React as React Frontend
    participant WASM as @calma/wasm-lib
    participant RPC as Solana RPC
    participant Program as calma Program (on-chain)

    User->>React: Enter borrow amount
    React->>WASM: amountToShares(amount, pool.totalBorrowed, pool.totalDebtShares)
    WASM-->>React: estimated debt shares
    React->>RPC: send borrow transaction
    RPC->>Program: borrow_handler(ctx, amount)
    Program->>Program: accrue_interest() via math
    Program->>Program: validate max_borrowable() via math
    Program->>Program: amount_to_shares() via math
    Program->>Program: update UserPosition.debt_shares
    Program->>Program: transfer lend tokens to user
    Program-->>RPC: transaction confirmed
    RPC-->>React: confirmation
    React-->>User: success toast + updated UI
```

---

## Rust Crate Dependencies

```mermaid
graph BT
    MATH["math\n─────────\ncompute_interest()\namount_to_shares()\nshares_to_amount()\nmax_borrowable()"]
    STATE["state\n─────────\nPool (49 KB, zero-copy)\nUserPosition (88 B)\nWithdrawalQueue"]
    WASM["bindings\n─────────\n#[wasm_bindgen]\nexports"]
    PROG["programs/calma\n─────────\n12 instructions"]

    STATE --> MATH
    WASM --> MATH
    WASM --> STATE
    PROG --> MATH
    PROG --> STATE
```

---

## On-Chain Program Instructions

```mermaid
graph TD
    subgraph Pool["Pool Management"]
        CREATE["create\nInitialise lending pool"]
    end

    subgraph Collateral["Collateral Side"]
        DEP_COL["deposit_collateral"]
        WITH_COL["withdraw_collateral"]
        BORROW["borrow"]
        REPAY["repay"]
    end

    subgraph Lending["Lending Side"]
        DEP_LENT["deposit_lent\nMint LP shares"]
        WITH_LENT["withdraw_lent\nBurn LP shares"]
        PROC_VQ["process_vault_queue\nAsync withdrawal processing"]
    end

    subgraph Flash["Flash Loans"]
        FL_BORROW["flash_borrow"]
        FL_REPAY["flash_repay"]
    end

    subgraph Queue["Async Queue"]
        PROC_Q["process_queue\nProcess pending borrows"]
    end

    BORROW --> PROC_Q
    DEP_LENT --> PROC_Q
    WITH_LENT --> PROC_VQ
```

---

## Account State

```mermaid
erDiagram
    Pool {
        pubkey authority
        pubkey collateral_mint
        pubkey lend_mint
        pubkey lp_mint
        u64 total_collateral_deposited
        u64 total_lend_deposited
        u64 total_borrowed
        u64 total_debt_shares
        i64 last_accrual_ts
        u64 total_lp_issued
        u8 ltv_percent
        UtilizationFeeConfig fee_config
        WithdrawalQueue withdrawal_queue
    }

    UserPosition {
        pubkey authority
        pubkey pool
        u64 collateral_deposited
        u64 debt_shares
        u8 bump
    }

    Pool ||--o{ UserPosition : "has many"
```

---

## Build Pipeline

```mermaid
flowchart LR
    MATH_SRC["crates/math\n(Rust source)"]
    STATE_SRC["crates/state\n(Rust source)"]
    WASM_SRC["crates/bindings\n(Rust source)"]
    APP_SRC["app/src\n(TypeScript source)"]

    ANCHOR_BUILD["anchor build\n→ programs/calma .so"]
    WASM_BUILD["wasm-pack build\n→ packages/wasm-lib/"]
    VITE_BUILD["vite build\n→ app/dist/"]

    MATH_SRC --> ANCHOR_BUILD
    STATE_SRC --> ANCHOR_BUILD
    MATH_SRC --> WASM_BUILD
    STATE_SRC --> WASM_BUILD
    WASM_SRC --> WASM_BUILD
    WASM_BUILD --> VITE_BUILD
    APP_SRC --> VITE_BUILD
```

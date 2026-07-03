# feed

The protocol's price interface. `jbl` and the WASM bindings only ever talk to
`feed`; they never read Pyth directly. Each `Feed` account is locked to one
price **source** at create time, with one setter instruction per source.

## Sources

- **Manual** (`source = 0`): an authority pushes prices via `set_value`. Feed IDs
  must be zeroed at create time.
- **Pyth** (`source = 1`): anyone may call `set_from_pyth`, which reads two
  `PriceUpdateV2` accounts (posted off-chain via the Pyth Pull / Hermes flow),
  validates owner + feed ID + staleness through the receiver SDK, normalizes to
  `PRICE_SCALE` (1e6), and stores both prices plus
  `last_updated_ts = min(collateral.publish_time, lend.publish_time)`. Both feed
  IDs must be non-zero, and `max_pyth_age_secs > 0`, at create time.

`get_state` (and its thin `get_value` wrapper) returns the decimal-adjusted
`ratio = collateral_price · 10^lend_decimals · PRICE_SCALE /
(lend_price · 10^collateral_decimals)` — the single `u64` that
`math::Oracle::price()` consumes.

## Ordering constraint: price the feed before creating a pool

`jbl::create` CPIs into `feed::get_state`, which requires `lend_price > 0`
(`compute_snapshot` rejects a zeroed feed with `ZeroPrice`). A freshly created
`Feed` has all prices at 0. **The feed must be populated before a pool is
created against it:**

1. `feed::create` (Manual or Pyth)
2. `feed::set_value` (Manual) **or** `feed::set_from_pyth` (Pyth) — sets the
   first non-zero price
3. `jbl::create` — reads the feed via CPI and applies the pool's own staleness
   guard

The integration tests follow this sequence; keep any new setup path aligned with
it.

## Account layout (Borsh, after the 8-byte discriminator)

`Feed` is defined in `crates/feed-state` and shared by the program, `jbl` (via
CPI types), and the WASM bindings. Fields serialize in declaration order:

| offset | field | bytes |
|--------|-------|-------|
| 0   | `state.collateral_price` (u64) | 8 |
| 8   | `state.lend_price` (u64) | 8 |
| 16  | `state.last_updated_ts` (i64) | 8 |
| 24  | `config.authority` (Pubkey) | 32 |
| 56  | `config.source` (u8) | 1 |
| 57  | `config.bump` (u8) | 1 |
| 58  | `config._pad` | 2 |
| 60  | `config.max_pyth_age_secs` (u32) | 4 |
| 64  | `config.collateral_feed_id` | 32 |
| 96  | `config.lend_feed_id` | 32 |
| 128 | `data.collateral_mint` (Pubkey) | 32 |
| 160 | `data.lend_mint` (Pubkey) | 32 |
| 192 | `data.collateral_decimals` (u8) | 1 |
| 193 | `data.lend_decimals` (u8) | 1 |
| 194 | `_reserved` | 30 |

Total body: 224 bytes. Prefer deserializing through `feed_state::Feed` over
hard-coded offsets so tests and clients can't drift from this layout.

## Build artifacts

`target/deploy/feed.so` and `target/idl/feed.json` are build outputs and are
**not** committed (`target/` is gitignored). Rebuild before running the LiteSVM
tests, which `include_bytes!` the compiled program:

```
cargo-build-sbf --manifest-path programs/feed/Cargo.toml   # or `anchor build`
cargo test -p feed
```

A stale `feed.so` surfaces as confusing "create failed" assertions because the
test's instruction data no longer matches the deployed program's signature.

## Known limitations

- **Confidence interval ignored.** `conf` is read but not enforced; wide spreads
  on thin assets can let borrows through on a noisy mid. Space is reserved in
  `_reserved` for a future soft cap.
- **Feed IDs are immutable post-create.** A typo requires recreating the feed.
  Both IDs are exposed through the WASM bindings so an operator can verify them.
- **`last_updated_ts = min(coll, lend)`** is conservative: the slower side gates
  the pair.

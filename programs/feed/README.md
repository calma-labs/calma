# feed

The protocol's price interface. `calma` and the WASM bindings only ever talk to
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

`calma::create` CPIs into `feed::get_state`, which requires `lend_price > 0`
(`compute_snapshot` rejects a zeroed feed with `ZeroPrice`). A freshly created
`Feed` has all prices at 0. **The feed must be populated before a pool is
created against it:**

1. `feed::create` (Manual or Pyth)
2. `feed::set_value` (Manual) **or** `feed::set_from_pyth` (Pyth) — sets the
   first non-zero price
3. `calma::create` — reads the feed via CPI and applies the pool's own staleness
   guard

The integration tests follow this sequence; keep any new setup path aligned with
it.

## Account layout (Borsh, after the 8-byte discriminator)

`Feed` is defined in `crates/feed-state` and shared by the program, `calma` (via
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
| 194 | `rules.max_conf_bps` (u16) | 2 |
| 196 | `rules.max_deviation_bps_per_hour` (u16) | 2 |
| 198 | `rules.ema_divergence_bps` (u16) | 2 |
| 200 | `rules.min_price` (u64) | 8 |
| 208 | `rules.max_price` (u64) | 8 |
| 216 | `rules._reserved` | 8 |

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

## Optional validation rules (Pyth source only)

`FeedRules` is set at `create` time and immutable. Every field uses `0` as the
"disabled" sentinel, so a zero-init `FeedRules` reproduces the pre-rules
behavior (staleness + source lock only). Rules apply to `set_from_pyth` only;
the Manual source stays fully authority-trusted.

| Field | Meaning | Comparison |
|-------|---------|------------|
| `max_conf_bps` | Reject wide Pyth spreads | `conf / price ≤ max_conf_bps` (bps) |
| `min_price` / `max_price` | Absolute normalized bounds | `min ≤ normalized ≤ max` (PRICE_SCALE units) |
| `ema_divergence_bps` | Guard against spot vs EMA drift | `|price − ema_price| / ema_price ≤ ema_divergence_bps` |
| `max_deviation_bps_per_hour` | Time-scaled circuit breaker | `|new − last| / last ≤ bps_per_hour × ceil(elapsed_hours)` |

The deviation budget scales with elapsed time because feeds may go long
stretches between updates; the effective ceiling is clamped at 10_000 bps
(100%), which gives even tightly configured feeds an eventual "any move
accepted" horizon. The first update after `create` skips the deviation check
because there is no prior price to compare against.

`create` rejects `min_price > max_price` when both are non-zero with
`InvalidRules`. Rule violations at update time surface as
`ConfidenceTooWide`, `PriceOutOfBounds`, `EmaDivergenceTooLarge`, or
`PriceDeviationTooLarge`.

Ideas reserved for later (space kept in `rules._reserved`):

- **Update cooldown** — minimum seconds between accepted updates.
- **Monotonic `publish_time`** — reject updates going backwards in time.
- **Verification-level requirement** — force Pyth `VerificationLevel::Full`
  before mainnet.

## Known limitations

- **Feed IDs are immutable post-create.** A typo requires recreating the feed.
  Both IDs are exposed through the WASM bindings so an operator can verify them.
- **`last_updated_ts = min(coll, lend)`** is conservative: the slower side gates
  the pair.
- **Rules are immutable post-create.** Tuning a rule requires recreating the
  feed (and the pool that depends on it).

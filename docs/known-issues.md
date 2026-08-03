# Known issues

Accepted risks and unresolved defects in the deployed protocol, kept here so
they are stated once and can be pointed at, rather than rediscovered.

**Scope.** These are properties of the on-chain programs. Anything a depositor
would want to know before entering a market is here. Entries marked **OPEN** are
defects with no fix and no accepted rationale — they are the ones that block a
mainnet deployment holding real value.

Every entry states what it is, why it is that way, and what a user or operator
has to do about it. Entries are numbered and never renumbered; a fixed entry is
marked RESOLVED with the commit rather than deleted.

| # | Issue | Class | Status |
|---|-------|-------|--------|
| [K-1](#k-1--there-is-no-liquidation) | No liquidation path | Design gap | **OPEN** |
| [K-2](#k-2--share-price-can-be-inflated-through-flash_repay) | Share-price inflation via `flash_repay` | Defect | RESOLVED |
| [K-3](#k-3--pyth-verification-level-is-not-checked) | Pyth verification level unchecked | Defect | RESOLVED |
| [K-4](#k-4--manual-feeds-are-fully-controlled-by-their-authority) | Manual feeds fully authority-controlled | Accepted | By design |
| [K-5](#k-5--market-creation-is-permissionless-and-so-is-the-choice-of-oracle) | Permissionless markets, caller-chosen oracle | Accepted | App-side mitigation pending |
| [K-6](#k-6--the-withdrawal-queue-is-finite-and-strictly-fifo) | Finite, strictly-FIFO withdrawal queue | Accepted | Mitigated |
| [K-7](#k-7--queueing-a-withdrawal-costs-the-caller-nothing) | Queue slots are free to occupy | Accepted | Mitigated |
| [K-8](#k-8--a-fast-price-move-can-freeze-a-market) | Deviation guard can freeze a market | Accepted | Bounded; config-dependent |
| [K-9](#k-9--the-pool-authority-can-reprice-outstanding-debt) | Authority can reprice open debt | Accepted | By design |
| [K-10](#k-10--the-protocol-fee-is-fixed-at-zero-and-has-no-setter) | Protocol fee fixed at 0, no setter | Accepted | By design |

---

## K-1 — There is no liquidation

**Status: OPEN. This is the issue that most limits what the protocol can safely
hold.**

`programs/calma/src/lib.rs` exposes eleven instructions and none of them closes
an unhealthy position. There is no `liquidate`, and `repay` is seed-bound to the
position's own `authority` (`instructions/repay.rs`), so a third party cannot
clear someone else's debt even voluntarily.

What follows from that:

- **Underwater debt is permanent.** Once a position's debt exceeds its
  collateral value, nothing in the protocol can close it. The collateral stays
  locked in the vault and the debt stays on the books.
- **The loss lands on lenders, unevenly.** `Core::accrue_interest`
  (`crates/math/src/core.rs`) credits accrued interest to `total_supply_assets`,
  so the LP share price keeps rising against debt that will never be repaid. The
  vault, however, only holds what has actually been paid in. Lenders who redeem
  early get real tokens at the inflated price; whoever is last holds the
  shortfall. The exit is first-come-first-served by construction.
- **`MAX_LTV_PERCENT` is 99** (`crates/math/src/lib.rs`). A market can be created
  one price tick away from insolvency, and nothing can close such a position.

The UI compounds this: `compute_liquidation_threshold` is exported from
`crates/math`, and `ManagePositionModal` / `LeverageModal` show users a health
factor and a liquidation price. A user will reasonably read that as "there is a
liquidation mechanism and it will fire here." There is not, and it will not.

`Pool::_reserved` (`crates/state/src/state/pool.rs`) holds 24 free `u64` slots
sized deliberately for the fields liquidation will need — close factor,
incentive bps, a liquidation LTV distinct from the borrow LTV, a paused flag, a
bad-debt counter — so adding it later needs no account migration.

**Until it exists:** treat every market as unsecured credit. Do not deploy
markets that hold value that matters, keep LTVs far below the ceiling, and do
not let the UI imply a liquidation that cannot happen.

---

## K-2 — Share price can be inflated through `flash_repay`

**Status: RESOLVED.** Kept here because the reasoning that made it possible —
a correct-sounding invariant that a later instruction quietly invalidated — is
the failure mode most likely to recur.

`Core::deposit_lent` (`crates/math/src/core.rs`) documents the ERC-4626
donation/inflation attack as closed, on the grounds that `total_supply_assets`
is internally accounted and never read from the vault's token balance — so a
direct token transfer into the vault cannot move the share price. That premise
is correct, and it is no longer sufficient.

`Core::flash_repay` credits `amount - principal` to `total_supply_assets`, where
`amount` is caller-chosen and only floor-checked (`amount < min_repay` fails;
nothing bounds it above). That is a donation channel straight into the internal
accounting the comment relies on. `flash_borrow`'s sysvar scan does not object
either — it accepts any `repay_amount >= min_repay`.

Sequence, verified against `math::Core`:

1. Attacker is the first LP and deposits 1 base unit → 1 share, 1 asset.
2. `flash_borrow(1)` then `flash_repay(2_000_001)` → still 1 share, now
   2_000_001 assets. Share price is 2_000_001 assets per share.
3. Victim deposits 3_000_000 → `3_000_000 × 1 / 2_000_001` floors to **1 share**.
4. Attacker redeems their share for 2_500_000, having spent 2_000_001 net.

Attacker profit 499_999; the victim's 3_000_000 is now worth 2_500_001. The
`lp == 0` guard in `deposit_lent` caps the theft at just under 50% of the
victim's deposit rather than 100%, but does not prevent it.

The attack needs `total_supply_shares` to be tiny, so it is a first-depositor
window, not a standing risk to an established pool — the donation an attacker
would have to make scales with the existing share count.

### How it was fixed

Two independent guards, because one of them being sufficient today is exactly
the assumption that failed the first time.

1. **`Core::flash_repay` requires exact repayment.** `amount` must equal
   `principal + fee`; a surplus is refused with `FlashLoanOverRepaid` rather
   than credited. Only the fee reaches `total_supply_assets`, and it is computed
   by the protocol rather than supplied by the caller — so the field is once
   again written only by protocol arithmetic.

   Refusing beats silently capping the credit at the fee: capping would leave
   the excess tokens in the vault outside the accounting, permanently
   unclaimable and a standing drift between vault balance and books. Exactness
   is safe to demand because `flash_min_repay` is a pure function of the
   borrowed amount, exported, and fixed when the borrow instruction is built —
   there is no race that could make hitting it unreliable.

2. **`Core::deposit_lent` refuses to seed below `MIN_SEED_LIQUIDITY`** (1000
   base units — chosen in base units so it assumes no decimal count; 0.001 of a
   token at 6 decimals). A one-share pool can no longer exist, so moving the
   share price means moving it against at least 1000 shares, raising the capital
   required by the same factor whatever channel a future change might open.

Regression tests: `flash_repay_refuses_a_surplus_above_principal_plus_fee` and
`a_seeder_cannot_inflate_the_share_price_against_a_later_depositor` in
`crates/math/src/core_tests.rs`. The original exploit was re-run against the
fixed code and blocked at step 1 by guard 2; re-seeded above the floor, it is
blocked at the flash-repay step by guard 1.

### What to take from it

`deposit_lent`'s comment asserted the inflation attack was closed, and its
stated reason — `total_supply_assets` is internally accounted, never read from
the vault balance — was true and remained true. The attack worked anyway,
because "internally accounted" had quietly come to include a field written from
a caller-chosen argument. **An invariant about a field is only as good as the
full set of writers to that field.** Anything that writes `total_supply_assets`,
`total_supply_shares`, `total_borrow_assets` or `total_borrow_shares` in future
must be bounded by protocol arithmetic, and the posture comment on
`deposit_lent` updated to match.

---

## K-3 — Pyth verification level is not checked

**Status: RESOLVED.**

`programs/feed/src/instructions/set_from_pyth.rs` reads the price with
`get_price_unchecked`. The comment there explains that this is deliberate for
*freshness* — the SDK's built-in age check works in whole seconds, which would
truncate the millisecond `max_age_ms` budget, so `check_max_age` applies it
instead. That reasoning is sound and the freshness gate is genuinely equivalent.

What the comment does not account for is that `get_price_unchecked` skips a
**second** check as well. In `pyth-solana-receiver-sdk`, `get_price_unchecked`
verifies only the `feed_id`. The `verification_level.gte(...)` check lives
exclusively in `get_price_no_older_than_with_custom_verification_level`, which
is the function `get_price_no_older_than` routes through with
`VerificationLevel::Full`. Bypassing the age check via `get_price_unchecked`
therefore bypasses the verification-level check with it.

The consequence: a `PriceUpdateV2` posted with
`VerificationLevel::Partial { num_signatures }` is accepted. Partial updates
carry a smaller guardian subset than `Full` requires, and posting them is
permissionless — anyone can create one and hand it to this instruction. The SDK
warns about exactly this: *"Lowering the verification level from `Full` to
`Partial` increases the risk of using a malicious price update."*

`set_from_pyth_push` is looser still: it reads `price_message` directly, so it
checks neither the verification level nor the embedded `feed_id`, and relies
entirely on the pinned account pubkey being a genuine Pyth-maintained sponsored
feed account.

### How it was fixed

Both instructions now assert `verification_level.gte(VerificationLevel::Full)`
on each leg before reading anything, and `check_max_age` still applies the
millisecond freshness budget — so the reason for avoiding
`get_price_no_older_than` is preserved without inheriting its omission. The
assertion is written out explicitly rather than left to whichever getter is
called, since that indirection is what hid the gap in the first place.

**One part could not be fixed.** On the push path the Pyth `feed_id` still is
not checked, because the `collateral_feed_id` / `lend_feed_id` config slots are
reused to hold the sponsored *account pubkey* — there is nowhere to store the
feed id to compare against. That path's guarantee therefore rests on the pinned
account genuinely being a Pyth-maintained sponsored feed account, chosen at feed
creation. Closing it properly needs a layout change to carry both values.

---

## K-4 — Manual feeds are fully controlled by their authority

**Status: accepted, by design. The risk is real and must be surfaced to users.**

`PriceSource::Manual` feeds are exactly what the name says: `feed::set_value`
writes whatever `collateral_price` and `lend_price` the feed authority signs
for, with none of the `FeedRules` gates applied. `create_handler` skips
`validate_rules` for `Manual` entirely, and `set_value` runs no confidence,
bounds, EMA-divergence or deviation check — only that both prices are non-zero.
`quote::set_price` has the same property.

This is intentional. A manual feed is a publisher of last resort: it exists for
pairs Pyth does not cover, for testing, and for markets whose operator *is* the
price authority and says so. Adding validation to it would not make it
trustworthy, only harder to reason about — the authority could satisfy any rule
it was also allowed to configure.

What follows is not a bug but must be stated plainly: **for any market whose
`feed_state` is a `Manual` feed, the feed authority can take every lend-side
deposit.** Set the collateral price arbitrarily high, borrow the whole vault
against dust collateral, walk away. There is no liquidation to unwind it (K-1)
and no rule that would have stopped the write.

**Operational requirement:** before a market holds value, verify its
`feed_state` resolves to a feed whose `config.source` is `Pyth` or `PythPush`,
not `Manual`, and not a `quote` provider. The app ships `useSetFeedManualValue`
and a feed-creation page, so manual feeds are a reachable production
configuration, not a test-only artifact.

---

## K-5 — Market creation is permissionless, and so is the choice of oracle

**Status: accepted, by design. App-side mitigation pending.**

`calma::create` is open to anyone, and it records `pool.feed_program` from the
*actual owner* of whatever account is passed as `feed_state`. There is no
canonical oracle program to compare against — that is the whole point of the
pluggable-oracle design (`crates/state/src/state/pool.rs`, `crates/interface`),
and it is what lets a Pyth-backed feed, a manual feed and a third-party TWAP all
serve markets without `calma` linking any of them.

The mint cross-check in `create_handler` does not close this. It proves the feed
*claims* to price this pair; a program the attacker wrote claims whatever it
likes. So anyone can stand up a real-looking market on genuine USDC/SOL mints
whose oracle is a program they control, and drain the lend side once lenders
arrive.

The protocol's answer is disclosure, not prevention: `feed_program`,
`feed_state`, `guard_state` and `rate_program` are all recorded on `Pool`, fixed
at creation, immutable afterwards, and inspectable by anyone before they
deposit. This is the Morpho-Blue posture — permissionless markets, risk pushed
to whoever chooses to enter one.

That only works if something helps users make that choice, which means **the
frontend is the entire safety boundary here.** A UI that discovers pools by mint
pair and renders them as equivalent hands users the attack.

**Pending:** the app will be updated to pin `feed_program` to the canonical feed
program (`orcdW2S1VR5kt8axERS4cJuiywxLPKo3qYYqN3Di5s4`) and to visibly separate
markets that do not match, rather than listing every discovered pool alike.
Until that ships, treat any pool not created by the operator as unvetted.

---

## K-6 — The withdrawal queue is finite and strictly FIFO

**Status: accepted, by design — but the part that made it dangerous is fixed.**

`WithdrawalQueue` (`crates/state/src/withdrawal_queue.rs`) is a fixed 1024-entry
ring buffer embedded in `Pool`, one slot reserved, so 1023 pending withdrawals
maximum. It is drained strictly head-first: `process_queue_entry` fails without
dequeueing when the vault cannot cover the head, so an entry never loses its
place — and nothing behind it can clear until it does.

### Fixed: queue occupancy no longer decides who gets paid

`withdraw_lent`'s immediate branch used to require `queue_is_empty`. One pending
entry therefore pushed every subsequent withdrawal onto the queued path however
liquid the pool was, a head that could not be paid held up exits that were fully
funded, and a full queue rejected them outright with `WithdrawalQueueFull`.

The gate is now liquidity: `borrowable_liquidity(vault_balance,
assets_in_queue)` — vault minus everything already owed to the queue, the same
reservation `borrow` respects. A lender is paid immediately whenever the vault
covers their exit *on top of* every queued claim, so queued lenders keep exactly
what they are owed (nothing behind them can spend it) while genuine surplus is
payable. With an empty queue `assets_in_queue` is 0 and this reduces to the
previous condition.

Covered by `immediate payout alongside a non-empty queue` in
`packages/test/withdrawal-queue.ts`.

### Still structural

- **A head that cannot be paid still blocks the queue behind it.** What changed
  is that being behind it no longer matters unless you actually need the
  reserved liquidity. If the pool is genuinely short, everyone waits — which is
  the honest outcome.
- **Capacity is still 1023, and a full queue still rejects new entries.** In a
  fully drained pool there is nothing to be paid from anyway; see K-7 for why
  filling it deliberately is no longer cheap.

The fixed size is deliberate: the queue lives inside the `Pool` account, which
is already ~49 KB and dominated by this array, and a growable queue would mean
either a per-entry account or a reallocation path on a zero-copy account. The
FIFO discipline is what makes a queued claim a *claim* — amounts are fixed when
the entry is enqueued (`Core::withdraw_lent_queued` burns the shares and moves
the assets to `assets_in_queue` at that moment), so out-of-order processing
would let a later lender jump a queue that was already priced.

**What this means in practice:** lend-side exit is not guaranteed to be prompt.
Under sustained high utilization the queue is the exit, it is bounded, and it
drains only as fast as borrowers repay.

---

## K-7 — Queueing a withdrawal costs the caller nothing

**Status: mitigated.**

A queue entry is a slot in an array that already exists inside `Pool`. It
carries no rent, no deposit and no fee — the caller pays a transaction fee and
nothing else. Combined with K-6, where a non-empty queue disables the immediate
path for everyone and a full queue rejects new withdrawals outright, that makes
occupying the 1023 slots a cheap way to stall lend-side exits until each entry
is individually processed.

**Mitigated:** zero-amount entries are now refused, at both the caller-facing
guard in `withdraw_lent_handler` and the backstop in `WithdrawalQueue::push`
(`ZeroValueWithdrawal`). Previously, shares worth zero lend tokens fell through
the `immediate` branch — which excluded them — onto the queued path, so an entry
that could never move a token still took a slot and still held the immediate
path shut. Taking a slot now requires having real value to claim.

**Mitigated:** one authority may now hold at most
`MAX_ENTRIES_PER_AUTHORITY` (8) pending entries. Filling 1023 slots therefore
needs ~128 distinct funded authorities rather than one, each with a real
deposit behind every entry.

The count is taken with `count_for_capped`, which stops as soon as the limit is
reached — so a party already at the cap does not force a walk of the whole
queue. The unavoidable cost is a caller with no entries and a near-full queue,
which compares 32 bytes per occupied slot against a bound of 1023.

**Mitigated indirectly, and more importantly:** the reason a full queue was
worth attacking is gone. Under K-6's fix, occupancy no longer gates anyone's
exit — a lender who can be paid from surplus is paid immediately whatever the
queue looks like. Filling the queue now only prevents *new entries being
queued*, which matters solely when the pool has no free liquidity, in which case
those entries could not have been paid regardless.

**Residual:** an attacker with enough capital and enough keypairs can still
occupy every slot. It is no longer close to free, and it no longer stalls
lenders who could otherwise be paid.

---

## K-8 — A fast price move can freeze a market

**Status: bounded at the low end; still depends on per-feed configuration.**

`rules::check_deviation` **rejects** an update that moves faster than
`max_deviation_bps_per_hour` allows, rather than clamping it. The budget scales
with elapsed time and is uncapped, so a large move is eventually accepted — but
not until enough hours have accrued.

During a fast move the feed therefore cannot be updated at all. Once
`price_ttl_ms` elapses (90 s by default), `hooks::oracle::read_feed` starts
returning `StaleOracle`, and every instruction that consults the oracle —
`borrow` and `withdraw_collateral` — fails until the deviation budget catches
up. `repay` and `deposit_collateral` keep working; they do not read the price.

The guard is doing its job: it is what stops a single bad print from repricing
every position in the market. The failure mode is the cost of that. It is also
the reason the old 10_000 bps clamp was removed — with a ceiling, a move past a
doubling was *permanently* unrepresentable and the feed froze forever rather
than for hours.

Note this interacts badly with K-1: a fast crash is exactly when positions go
underwater, and it is also when the market is least able to react.

### What was changed

There is no fix that makes this go away — the trade-off between admitting a bad
print and refusing a real move is the guard's entire purpose, and rejecting is
the correct side of it. What can be removed is the pathological setting:
`validate_rules` now refuses a non-zero `max_deviation_bps_per_hour` below
`MIN_DEVIATION_BPS_PER_HOUR` (1000 bps = 10% per hour). Below that the value is
almost certainly a mistake rather than a risk appetite, and the right time to
find out is at feed creation rather than during a crash. `create` is the only
write site — there is no rules setter — so this covers every feed.

That bounds the freeze; it does not prevent it. At the floor, a halving is
admitted after roughly five hours. **A tight budget is still capable of halting
a market for hours, and this interacts badly with K-1: a fast crash is exactly
when positions go underwater, and also when the market is least able to react.**

**Operational requirement, unchanged:** set `max_deviation_bps_per_hour`
generously, or to `0` (disabled), for any feed on a volatile pair. A budget
tight enough to catch a bad print is usually tight enough to freeze the market
in a real crash. Feeds created before this validation existed are not
retroactively checked — audit any that are already live.

---

## K-9 — The pool authority can reprice outstanding debt

**Status: accepted, by design.**

`calma::create` binds the market's IRM authority to the pool creator
(`hooks::irm::check_irm_authority`), and `irm::set_fee_points` is callable by
that authority at any time, with no timelock and no notice. Rate points are
capped at 5000 bps (50% APR) by `validate_rate_points`, and `get_fee_bps` clamps
the interpolated result to the same ceiling — so a steep curve cannot smuggle a
higher rate past the bound — but anything up to that ceiling can be set at will
and applies to already-open positions from the next accrual.

The check exists to prevent something worse: `irm::initialize` is permissionless
and first-come-first-served on the `["irm_config", pool]` PDA, so without it a
market could be created already bound to a curve someone else controlled, with
nothing on `Pool` revealing it. Binding the curve to the market's own creator is
the lesser risk, not the absence of one.

The curve is on-chain and readable, and `rate_program` / `irm_state` are pinned
on `Pool` at creation. Borrowers can always exit: `repay` is never gated by the
guard and never reads the oracle.

---

## K-10 — The protocol fee is fixed at zero and has no setter

**Status: accepted, by design.**

`Market::fee` is stamped to `0` in `create_handler` and there is no instruction
that changes it — the `set_fee` entrypoint was removed, and its error variant
survives as a RESERVED placeholder in `crates/state/src/error.rs` to keep Anchor
error numbering stable. As shipped, no interest is ever skimmed.

The field and the accrual arithmetic that reads it are kept in place so a fee
can be reintroduced without a layout change. Two things follow:

- **The protocol earns nothing.** All borrower-paid interest goes to lenders.
- **The fee-share path is dead code on a live deployment.** The `fee_bps > 0`
  branch of `Core::accrue_interest`, `Market::accrued_fee_shares`, and the whole
  `claim_fees` instruction are unreachable while `fee == 0`. They are exercised
  by unit tests only, never by a deployed pool. Anything that reintroduces a
  setter is turning on code that has never run in production and should be
  reviewed as new.

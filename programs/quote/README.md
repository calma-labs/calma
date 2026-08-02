# `quote` — a combined price and rate provider

A market in `calma` depends on two external things: an **oracle** it reads a
price from, and a **rate model** it asks a borrow rate of. Neither is a fixed
program. A market records which program serves each role when it is created and
is pinned to that choice for life, so anything satisfying the interfaces below
can back a market.

`programs/feed` and `programs/irm` are the reference implementations, one role
each. This program is a second, independent implementation that fills **both
roles from a single account** — which is also what makes it the repo's proof
that the interfaces are real interfaces and not descriptions of one program.

## The price interface

Own an account whose bytes, directly after the 8-byte discriminator, are an
`interface::PriceFeedHeader`. That is the entire contract.

`calma` reads the header out of the account with no CPI, and trusts it because
the account's owner equals the program the market recorded as `feed_program` (a
value taken from the account's own owner at creation, so it cannot disagree with
reality). Two consequences worth knowing:

- **The discriminator is skipped, not checked.** With pluggable providers each
  implementation names its account type differently, so there is no single value
  to compare against. Nothing is lost: the market pins one exact address, so
  there is no second account of another type for such a check to catch.
- **Bytes below the header are ignored.** This is what lets one account carry
  rate-model fields underneath a price header.

The header owns its own staleness budget in `price_ttl_ms` — the provider knows
its update cadence, and the markets pricing against it do not. `0` rejects every
consumer, so it is refused at write time rather than accepted as "no limit".

## The rate interface

This one is a CPI, because a borrow rate depends on live utilization and so is a
computation rather than stored state. Three things bind an implementation, none
of which are visible from `calma`'s source:

| Requirement | Detail |
|---|---|
| **Instruction names** | `calma` dispatches to `pool.rate_program` at runtime but encodes the call using the reference program's generated helpers, so what actually travels is Anchor's discriminator — `sha256("global:borrow_rate")[..8]` and `sha256("global:check_authority")[..8]`. The names must match character for character. |
| **Signatures** | `borrow_rate(utilization_bps: u64) -> Result<u32>` and `check_authority(authority: Pubkey) -> Result<()>`. The `u32` comes back through Anchor's return-data encoding. |
| **Account order** | Exactly two accounts — the rate account, then the pool — neither signer nor writable. Field *names* are free; position is the contract. |
| **PDA seeds** | The rate account must be `["irm_config", pool]` under your own program id. `calma::create` re-derives it and refuses anything else. |

Nothing constrains the **model**. `irm` interpolates a 2–4 point piecewise-linear
curve; this program returns a constant at every utilization. `calma` consumes a
`u32` and never learns which. A provider reading a governance feed, or a
different curve shape entirely, is the same interface.

`check_authority` exists because `calma` cannot read your layout to find out who
controls the curve. See the security note below for why it has to ask.

## One account, both roles

`Provider` sits at `["irm_config", pool]` — the address the rate role requires —
and leads with the price header the oracle role requires:

```rust
#[account]
pub struct Provider {
    pub header: PriceFeedHeader,  // read directly by calma
    pub pool: Pubkey,
    pub authority: Pubkey,
    pub bump: u8,
    pub flat_rate_bps: u32,       // reached over CPI by calma
}
```

A market is created naming the same pubkey for both `feed_state` and
`irm_state`. `calma::create` then reads the account *and* CPIs the program that
owns it, in the same instruction.

## Security note: permissionless, first-come-first-served

`initialize` may be called by anyone for any pool, exactly as `irm::initialize`
may be, and whoever calls it names themselves `authority` for the life of the
market. That is not an oversight, and it is only safe because of the
`check_authority` CPI: `calma::create` asks the provider whether the market's own
creator controls it, so a market whose provider was claimed by someone else
**cannot be created** — rather than being created and quietly controlled by that
someone.

The authority can move both the price and the rate afterwards. A market's
depositors are therefore trusting whoever holds it, which is why `Pool` records
`feed_program` and `rate_program` as inspectable fields: they name who is trusted
to price and rate the market, and neither can change after creation.

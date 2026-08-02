---
paths:
  - "programs/*/Cargo.toml"
  - "crates/*/Cargo.toml"
  - "Cargo.toml"
---

# Dependency policy for on-chain crates

## Policy: a program links no other program

`programs/calma/Cargo.toml` must not list another **program** crate under
`[dependencies]`. Today that means none of `feed`, `irm`, `guard`, `faucet` or
`quote` — and the same applies to any provider added later.

**Why.** A market reaches every external program it uses through a pubkey
recorded on its own `Pool` account, chosen at creation and pinned for life:
`feed_program`, `rate_program`, `guard_program`. Linking one implementation
privileges it at compile time and contradicts that — the protocol would ship
knowing one provider's types while claiming all are equal. It also makes the
lending program's binary depend on the release cadence of programs it should be
indifferent to.

Verify with `cargo tree`, not by reading the manifest:

```bash
# `tail -n +2` drops calma's own root line, which is itself under programs/.
cargo tree -p calma --edges normal --depth 1 | tail -n +2 | grep programs/

# A provider should have no dependents at all: the output is just its own line.
cargo tree -i -p <provider> --edges normal,dev
```

Both are expected to come back empty (the second, bar the provider itself). Note
`cargo tree -i` needs `--edges normal,dev` to catch a dev-dependency edge, which
is exactly the kind that gets added "just for a test" and then stays.

## Talking to another program without linking it

Encode the CPI by hand. The pattern lives in
`programs/calma/src/hooks/irm.rs` and `hooks/guard.rs`; copy it rather than
inventing a variant.

1. **Hardcode the discriminator** as a `const [u8; 8]`, with the instruction name
   in a doc comment. Anchor derives it as `sha256("global:<name>")[..8]`.
2. **Assert the derivation in a `#[cfg(test)] mod tests`** in the same file,
   using `sha2` (already a dev-dependency of `calma`). This is the safety net
   that replaces the compiler — without it a typo is a runtime failure on
   mainnet, not a build error.
3. **Build the `Instruction` explicitly** — `program_id` from the account the
   market recorded, `accounts` as an ordered `Vec<AccountMeta>` with
   signer/writable flags spelled out, `data` as discriminator + borsh args.
   Position, not field name, is what binds.
4. **Attribute return data before believing it.** `get_return_data()` reads a
   single global slot, so a value left by an earlier CPI in the same transaction
   would otherwise be read as this program's answer:

   ```rust
   let (returning_program, bytes) = get_return_data().ok_or(..)?;
   require_keys_eq!(returning_program, rate_program.key(), ..);
   ```

Borsh for the primitives involved is little-endian in declaration order, with no
padding: `u16`/`u32`/`u64` are their LE bytes, `Pubkey` is its 32, a fieldless
enum is one byte, `Vec<T>` is a `u32` count then the elements.

## Pin programs on the account, never against a constant

If a program id needs to be trusted, record it on `Pool` at creation and compare
against the stored value on every use. Do **not** reintroduce a
`require!(x.key() == some_crate::ID)` check — that is a program dependency wearing
a different hat.

`Pool` reserves growth room (`_reserved: [u64; N]`) precisely for this; a `Pubkey`
consumes 4 slots and leaves the account's size, and therefore every live
account's rent and layout, untouched. `guard_program` was added this way.

Both halves matter for a CPI-based check: pinning only the *state* account lets a
caller pass their own program alongside it and have it answer `Ok` without
reading anything. `calma` hands the account over and believes the reply — it
cannot tell an implementation from an impostor.

## What is fine

- **Library crates under `crates/`** — `math`, `state`, `interface`,
  `feed-state`, `irm-state`. They hold types and arithmetic, not programs.
  `interface` is *the* contract that lets providers be swapped, so depending on
  it is the point rather than a violation.
- **Provider programs depending on `crates/`** — `feed` → `feed-state`,
  `irm` → `irm-state`, `quote` → `interface`. What a provider must not do is
  depend on `calma`.
- **`[dev-dependencies]` on the reference programs.** Test fixtures stand up real
  `feed` / `irm` / `guard` / `faucet` instances, and hand-rolling their encoding
  to prove a point the shipped binary already proves is not worth the noise.

  The exception is an **alternative** provider whose whole claim is that `calma`
  has never seen it — `quote` is kept out even of dev-dependencies, and
  `programs/calma/tests/test_alternative_provider.rs` builds its instructions
  from the program id. A test cannot honestly demonstrate integrating an unknown
  program while linking it.

## Package naming gotcha

A workspace package whose name collides with a crates.io crate already in the
graph makes `cargo -p <name>` ambiguous:

```
error: specification `quote` is ambiguous
help: re-run with one of: quote@0.1.0, quote@1.0.45
```

Give the `[package]` an unambiguous name and keep the program's real name on
`[lib]`, which is what determines the crate name in code, `target/deploy/<name>.so`
and the `Anchor.toml` key. `programs/quote` does this: package `quote-provider`,
lib `quote`.

## Adding a program

New programs are picked up automatically (`members = ["programs/*", "crates/*"]`),
but the id has to be wired by hand: `declare_id!`, both `[programs.*]` sections in
`Anchor.toml`, and the `link-keys` script. `anchor test` launches the validator
with the `Anchor.toml` ids, so a mismatch there is silent locally and only bites
on a real deploy.

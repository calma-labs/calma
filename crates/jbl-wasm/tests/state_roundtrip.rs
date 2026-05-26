//! Integration tests that build the `jbl-wasm` WASM module, load it via
//! `wasmtime`, copy serialised account bytes into WASM linear memory, invoke
//! the exported parse functions, read the decoded struct back from memory, and
//! assert field-level round-trip fidelity.
//!
//! The `build_and_load()` fixture calls `cargo build --target
//! wasm32-unknown-unknown` so the binary is always up-to-date.  Subsequent
//! runs are fast because cargo's incremental compilation is a no-op when
//! nothing has changed.

use anchor_lang::prelude::Pubkey;
use bytemuck::{bytes_of, Pod, Zeroable};
use jbl_state::{Pool, RateHedgeMatch, RateHedgeOffer, UserPosition};
use wasmtime::{Engine, Instance, Linker, Module, Store};

// ── WASM linear-memory layout ─────────────────────────────────────────────────
//
// Pages are 64 KiB each.  Page 0 is reserved for the WASM data segment /
// stack.  We place test I/O in pages 1 and 2.
//
//   INPUT_OFFSET  = 0x1_0000 ( 65 536 B)  <- wire bytes written by the host
//   OUTPUT_OFFSET = 0x2_0000 (131 072 B)  <- decoded struct written by WASM
//
// Pool is the largest struct (41 184 B).  It fits comfortably in page 2
// (which ends at 196 607 B), so MIN_PAGES = 3 suffices.

const INPUT_OFFSET: usize = 0x1_0000;
const OUTPUT_OFFSET: usize = 0x2_0000;
const MIN_PAGES: u64 = 3;

/// A recognisable discriminator prefix for the wire format.
const DISC: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];

/// Build the Anchor wire format: 8-byte discriminator ++ raw struct bytes.
fn wire<T: Pod>(value: &T) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + core::mem::size_of::<T>());
    buf.extend_from_slice(&DISC);
    buf.extend_from_slice(bytes_of(value));
    buf
}

// ── wasmtime fixture ──────────────────────────────────────────────────────────

fn build_and_load() -> (Store<()>, Instance) {
    // Compile the WASM binary (no-op when nothing changed).
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent().unwrap().parent().unwrap();

    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "jbl-math-wasm",
        ])
        .current_dir(workspace)
        .status()
        .expect("failed to invoke `cargo build`");

    assert!(
        status.success(),
        "WASM build failed -- ensure the `wasm32-unknown-unknown` target is installed \
         (`rustup target add wasm32-unknown-unknown`)"
    );

    let wasm_path = workspace.join("target/wasm32-unknown-unknown/debug/jbl_math_wasm.wasm");
    let wasm_bytes = std::fs::read(&wasm_path)
        .expect("WASM binary not found after a successful build -- this is a bug");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("invalid WASM module");
    let mut store = Store::new(&engine, ());

    // Stub the four wasm-bindgen runtime imports that the cdylib requires.
    // Our `#[no_mangle]` parse functions never call them; they exist only for
    // the `#[wasm_bindgen]` math exports.
    let mut linker: Linker<()> = Linker::new(&engine);
    linker
        .func_wrap(
            "__wbindgen_placeholder__",
            "__wbindgen_describe",
            |_: i32| {},
        )
        .unwrap();
    linker
        .func_wrap::<_, ()>(
            "__wbindgen_placeholder__",
            "__wbg___wbindgen_throw_9c75d47bf9e7731e",
            |_: i32, _: i32| panic!("wasm-bindgen throw called"),
        )
        .unwrap();
    linker
        .func_wrap(
            "__wbindgen_externref_xform__",
            "__wbindgen_externref_table_grow",
            |_: i32| -> i32 { 0 },
        )
        .unwrap();
    linker
        .func_wrap(
            "__wbindgen_externref_xform__",
            "__wbindgen_externref_table_set_null",
            |_: i32| {},
        )
        .unwrap();

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("WASM instantiation failed");

    // Ensure the linear memory covers INPUT and OUTPUT regions.
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let current = memory.size(&store);
    if current < MIN_PAGES {
        memory
            .grow(&mut store, MIN_PAGES - current)
            .expect("failed to grow WASM memory");
    }

    (store, instance)
}

/// Write `wire_bytes` at `INPUT_OFFSET`, call `fn_name(input_ptr, input_len,
/// output_ptr)`, read `size_of::<T>()` bytes from `OUTPUT_OFFSET`, and return
/// the decoded value.  Panics if the WASM function returns `-1`.
fn wasm_call<T: Pod + Zeroable>(
    store: &mut Store<()>,
    instance: &Instance,
    fn_name: &str,
    wire_bytes: &[u8],
) -> T {
    let memory = instance.get_memory(&mut *store, "memory").unwrap();

    // Write input bytes into WASM memory.
    memory.data_mut(&mut *store)[INPUT_OFFSET..INPUT_OFFSET + wire_bytes.len()]
        .copy_from_slice(wire_bytes);

    // Call the exported parse function.
    let f = instance
        .get_typed_func::<(i32, i32, i32), i32>(&mut *store, fn_name)
        .unwrap_or_else(|_| panic!("WASM export `{fn_name}` not found"));

    let ret = f
        .call(
            &mut *store,
            (
                INPUT_OFFSET as i32,
                wire_bytes.len() as i32,
                OUTPUT_OFFSET as i32,
            ),
        )
        .unwrap();

    assert_eq!(ret, 0, "`{fn_name}` returned -1 (parse failed)");

    // Read the decoded struct back from WASM memory.
    let size = core::mem::size_of::<T>();
    let bytes = memory.data(&*store)[OUTPUT_OFFSET..OUTPUT_OFFSET + size].to_vec();
    bytemuck::pod_read_unaligned::<T>(&bytes)
}

// ── UserPosition ──────────────────────────────────────────────────────────────

#[test]
fn user_position_roundtrip_wasm() {
    let (mut store, instance) = build_and_load();

    let mut original: UserPosition = Zeroable::zeroed();
    original.authority = Pubkey::new_unique();
    original.pool = Pubkey::new_unique();
    original.collateral_deposited = 5_000_000;
    original.debt_shares = 250;
    original.bump = 3;

    let parsed = wasm_call::<UserPosition>(
        &mut store,
        &instance,
        "wasm_parse_user_position",
        &wire(&original),
    );

    assert_eq!(parsed.authority, original.authority);
    assert_eq!(parsed.pool, original.pool);
    assert_eq!(parsed.collateral_deposited, original.collateral_deposited);
    assert_eq!(parsed.debt_shares, original.debt_shares);
    assert_eq!(parsed.bump, original.bump);
}

// ── RateHedgeOffer ────────────────────────────────────────────────────────────

#[test]
fn rate_hedge_offer_roundtrip_wasm() {
    let (mut store, instance) = build_and_load();

    let mut original: RateHedgeOffer = Zeroable::zeroed();
    original.pool = Pubkey::new_unique();
    original.authority = Pubkey::new_unique();
    original.amount = 10_000_000;
    original.fixed_rate_bps = 500;
    original.min_duration = 86_400;
    original.max_duration = 2_592_000;
    original.collateral_deposited = 1_000_000;
    original.locked_tokens = 50_000;
    original.bump = 7;

    let parsed = wasm_call::<RateHedgeOffer>(
        &mut store,
        &instance,
        "wasm_parse_rate_hedge_offer",
        &wire(&original),
    );

    assert_eq!(parsed.pool, original.pool);
    assert_eq!(parsed.authority, original.authority);
    assert_eq!(parsed.amount, original.amount);
    assert_eq!(parsed.fixed_rate_bps, original.fixed_rate_bps);
    assert_eq!(parsed.min_duration, original.min_duration);
    assert_eq!(parsed.max_duration, original.max_duration);
    assert_eq!(parsed.collateral_deposited, original.collateral_deposited);
    assert_eq!(parsed.locked_tokens, original.locked_tokens);
    assert_eq!(parsed.bump, original.bump);
}

// ── RateHedgeMatch ────────────────────────────────────────────────────────────

#[test]
fn rate_hedge_match_roundtrip_wasm() {
    let (mut store, instance) = build_and_load();

    let mut original: RateHedgeMatch = Zeroable::zeroed();
    original.offer = Pubkey::new_unique();
    original.user_position = Pubkey::new_unique();
    original.amount = 8_000_000;
    original.upfront_fee = 40_000;
    original.initial_debt_shares = 820;
    original.start_ts = 1_716_000_000;
    original.duration = 604_800;
    original.bump = 2;

    let parsed = wasm_call::<RateHedgeMatch>(
        &mut store,
        &instance,
        "wasm_parse_rate_hedge_match",
        &wire(&original),
    );

    assert_eq!(parsed.offer, original.offer);
    assert_eq!(parsed.user_position, original.user_position);
    assert_eq!(parsed.amount, original.amount);
    assert_eq!(parsed.upfront_fee, original.upfront_fee);
    assert_eq!(parsed.initial_debt_shares, original.initial_debt_shares);
    assert_eq!(parsed.start_ts, original.start_ts);
    assert_eq!(parsed.duration, original.duration);
    assert_eq!(parsed.bump, original.bump);
}

// ── Pool ──────────────────────────────────────────────────────────────────────

#[test]
fn pool_zeroed_roundtrip_wasm() {
    let (mut store, instance) = build_and_load();

    let original: Pool = Zeroable::zeroed();

    let parsed = wasm_call::<Pool>(&mut store, &instance, "wasm_parse_pool", &wire(&original));

    assert_eq!(parsed.lend_mint, original.lend_mint);
    assert_eq!(parsed.collateral_mint, original.collateral_mint);
    assert_eq!(parsed.total_borrowed, original.total_borrowed);
    assert_eq!(parsed.total_debt_shares, original.total_debt_shares);
}

// ── error path ────────────────────────────────────────────────────────────────

#[test]
fn parse_failure_returned_from_wasm() {
    let (mut store, instance) = build_and_load();

    // 7 bytes is fewer than the 8-byte discriminator; parsing must fail.
    let short = [0xAAu8; 7];
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    memory.data_mut(&mut store)[INPUT_OFFSET..INPUT_OFFSET + short.len()].copy_from_slice(&short);

    let f = instance
        .get_typed_func::<(i32, i32, i32), i32>(&mut store, "wasm_parse_user_position")
        .unwrap();

    let ret = f
        .call(
            &mut store,
            (
                INPUT_OFFSET as i32,
                short.len() as i32,
                OUTPUT_OFFSET as i32,
            ),
        )
        .unwrap();

    assert_eq!(ret, -1, "truncated input must return -1 from WASM");
}

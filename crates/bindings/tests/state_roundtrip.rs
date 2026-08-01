//! Integration tests that build the `bindings` WASM module, load it via
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
use state::{Pool, UserPosition};
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

    const CRATE_NAME: &str = "bindings";

    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            CRATE_NAME,
        ])
        .current_dir(workspace)
        .status()
        .expect("failed to invoke `cargo build`");

    assert!(
        status.success(),
        "WASM build failed -- ensure the `wasm32-unknown-unknown` target is installed \
         (`rustup target add wasm32-unknown-unknown`)"
    );

    let wasm_path = workspace.join(format!(
        "target/wasm32-unknown-unknown/debug/{}.wasm",
        CRATE_NAME.replace('-', "_")
    ));
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

    // The `#[wasm_bindgen]` math exports pull in further runtime imports whose
    // mangled names carry build-dependent hash suffixes (e.g. the `Date.now()`
    // binding `__wbg_now_*` used by `BrowserClock`). The parse functions under
    // test never call them, so trap on any that remain unresolved rather than
    // enumerating each brittle name.
    linker
        .define_unknown_imports_as_traps(&module)
        .expect("failed to stub remaining WASM imports");

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

// ── Pool ──────────────────────────────────────────────────────────────────────

#[test]
fn pool_zeroed_roundtrip_wasm() {
    let (mut store, instance) = build_and_load();

    let original: Pool = Zeroable::zeroed();

    let parsed = wasm_call::<Pool>(&mut store, &instance, "wasm_parse_pool", &wire(&original));

    assert_eq!(parsed.lend_mint, original.lend_mint);
    assert_eq!(parsed.collateral_mint, original.collateral_mint);
    assert_eq!(
        parsed.market.total_borrow_assets,
        original.market.total_borrow_assets
    );
    assert_eq!(
        parsed.market.total_borrow_shares,
        original.market.total_borrow_shares
    );
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

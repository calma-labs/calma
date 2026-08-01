//! Raw-pointer C-ABI exports consumed by the `wasmtime` integration tests.
//!
//! Each function receives the address and length of the raw account bytes in
//! WASM linear memory, parses them, writes the decoded struct to a
//! caller-allocated output buffer, and returns `0` on success or `-1` if
//! parsing fails (wrong length, etc.).
//!
//! Pointer arguments are `i32` because WASM32 linear-memory addresses are
//! unsigned 32-bit values, conventionally represented as `i32` in WASM ABI.

use crate::state::{PoolAccount, UserPositionAccount};
use bytemuck::bytes_of;

macro_rules! export_parse {
    ($fn_name:ident, $wrapper:ty) => {
        /// # Safety
        /// `data_ptr..data_ptr+data_len` and `out_ptr..out_ptr+size_of_inner`
        /// must be valid, non-overlapping regions of WASM linear memory.
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(data_ptr: i32, data_len: i32, out_ptr: i32) -> i32 {
            let bytes = core::slice::from_raw_parts(data_ptr as *const u8, data_len as usize);
            match <$wrapper>::from_bytes(bytes) {
                Some(account) => {
                    let src = bytes_of(&account.0);
                    core::ptr::copy_nonoverlapping(src.as_ptr(), out_ptr as *mut u8, src.len());
                    0
                }
                None => -1,
            }
        }
    };
}

export_parse!(wasm_parse_user_position, UserPositionAccount);
export_parse!(wasm_parse_pool, PoolAccount);

//! Raw-ABI WebAssembly binding of `tradelayer-core`.
//!
//! One JSON request per call, one JSON response: `{"ok":true,...}` or
//! `{"ok":false,"code":"...","message":"..."}`. Operations whose name starts with `pg_` go to the
//! economic [`playground`]; all others go to [`codec`]. The same [`handle`] runs natively in
//! tests and benchmarks and inside the browser or Node. `unsafe` exists only in the pointer glue
//! below; the core crate forbids it.
//!
//! ABI (no imports): exports `tl_alloc(len) -> ptr`, `tl_free(ptr, len)` and
//! `tl_call(req_ptr, req_len) -> ptr`, where the result buffer is `[len: u32 LE][json bytes]`
//! and is released with `tl_free(ptr, 4 + len)`.

pub mod codec;
pub mod json;
pub mod playground;

use json::{quote, Obj, OpError};
use tradelayer_core::client::RpcValue;

/// Requests above this size are refused before parsing.
pub const MAX_REQUEST_BYTES: usize = 16 << 20;

/// Handles one JSON request and returns the JSON response bytes.
pub fn handle(request: &[u8]) -> Vec<u8> {
    match run(request) {
        Ok(body) => body.into_bytes(),
        Err(e) => Obj::new()
            .bool("ok", false)
            .raw("code", quote(&e.code))
            .raw("message", quote(&e.message))
            .build()
            .into_bytes(),
    }
}

fn run(request: &[u8]) -> json::Res<String> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(OpError::bad("request too large"));
    }
    let text = core::str::from_utf8(request).map_err(|_| OpError::bad("request is not UTF-8"))?;
    let req = RpcValue::parse(text).map_err(|_| OpError::bad("request is not JSON"))?;
    let op = json::str_field(&req, "op")?;
    // Each handler returns the members of the success object; `ok` is added here.
    let members = if op.starts_with("pg_") {
        playground::handle(&op, &req)?
    } else {
        codec::handle(&op, &req)?
    };
    Ok(members.with_ok().build())
}

#[cfg(target_arch = "wasm32")]
mod abi {
    /// Allocates `len` bytes for a request and returns the pointer.
    #[no_mangle]
    pub extern "C" fn tl_alloc(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len.max(1)].into_boxed_slice()).cast::<u8>()
    }

    /// Releases a buffer from [`tl_alloc`] or [`tl_call`].
    ///
    /// # Safety
    /// `ptr` must come from `tl_alloc(len)` or be a `tl_call` result of total size `len`.
    #[no_mangle]
    pub unsafe extern "C" fn tl_free(ptr: *mut u8, len: usize) {
        // SAFETY: the caller passes a pointer and exact length of a boxed slice allocated here.
        drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len.max(1))) });
    }

    /// Runs one request and returns `[len: u32 LE][json]`.
    ///
    /// # Safety
    /// `ptr` must be valid for `len` bytes (a buffer from [`tl_alloc`]).
    #[no_mangle]
    pub unsafe extern "C" fn tl_call(ptr: *const u8, len: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` is valid for `len` bytes.
        let request = unsafe { core::slice::from_raw_parts(ptr, len) };
        let body = crate::handle(request);
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        Box::into_raw(out.into_boxed_slice()).cast::<u8>()
    }
}

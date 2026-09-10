#![no_std]

use engram_shared::{error::*, heap, Cursor};

#[unsafe(export_name = "alloc")]
pub extern "C" fn alloc(size: u32) -> u32 {
    heap::alloc(size)
}

// Free a staging buffer previously returned by `alloc`. The executor creates
// a fresh instance per run, so nothing calls this yet.
#[unsafe(export_name = "dealloc")]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    heap::dealloc(ptr, size)
}

// out_len_ptr: uniform guest ABI — pointer to a host-allocated heap slot
// where variable-length outputs store their length (BE u32). Count's output
// is a fixed 8 bytes, so the slot is accepted and ignored.
#[unsafe(export_name = "run")]
pub extern "C" fn run(
    input_ptr: u32, input_len: u32,
    params_ptr: u32, params_len: u32,
    out_ptr: u32,
    _out_len_ptr: u32,
) -> i32 {
    unsafe {
        let mut param_cursor = Cursor::new(params_ptr, params_len);

        // version byte at offset 0 must be 0x01 (Spec.md §0). Peek without
        // advancing: the field peeks below are relative to offset 0.
        match param_cursor.peek_bytes(1) {
            Ok(v) => {
                if v[0] != 0x01 {
                    return E_INVALID_PARAM;
                }
            }
            Err(()) => return E_INVALID_PARAM,
        }
        // Params blob must be exactly 11 bytes (Spec.md §3); trailing bytes
        // are always rejected (§0).
        if params_len > 11 {
            return E_TRAILING_DATA;
        }
        if params_len < 11 {
            return E_INVALID_PARAM;
        }

        let record_offset: u32;
        let field_offset: u32;
        match param_cursor.peek_field(1, 4, 0) {
            Ok(v) => record_offset = v as u32,
            Err(()) => return E_INVALID_PARAM,
        };
        match param_cursor.peek_field(5, 4, 0) {
            Ok(v) => field_offset = v as u32,
            Err(()) => return E_INVALID_PARAM,
        };
        if param_cursor.advance(9).is_err() {
            return E_MALFORMED_INPUT;
        };
        let field_width: u8;
        match param_cursor.read_bytes(1) {
            Ok(v) => field_width = v[0],
            Err(()) => return E_INVALID_PARAM,
        }
        if field_width != 4 && field_width != 8 {
            return E_INVALID_PARAM;
        }
        if input_len % (record_offset as u32) != 0 {
            return E_MALFORMED_INPUT;
        }
        let signed: u8;
        match param_cursor.read_bytes(1) {
            Ok(v) => signed = v[0],
            Err(()) => return E_INVALID_PARAM,
        }
        if signed > 1 {
            return E_INVALID_PARAM;
        }

        let mut input_cursor = Cursor::new(input_ptr, input_len);
        let mut result: i64 = 0;
        while !input_cursor.at_end() {
            let value = match input_cursor.peek_field(field_offset, field_width, signed) {
                Ok(v) => v,
                Err(()) => return E_MALFORMED_INPUT,
            };
            result = result.wrapping_add(value);
            if input_cursor.advance(record_offset).is_err() {
                return E_MALFORMED_INPUT;
            }
        }
        let out = core::slice::from_raw_parts_mut(out_ptr as *mut u8, 8);
        out.copy_from_slice(&result.to_be_bytes());
        return E_OK;
    }
}

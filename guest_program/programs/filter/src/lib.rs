#![no_std]

use engram_shared::{error::*, heap, Cursor};

// The host stages params/input/out/length-slot through the exported `alloc`,
// which is dlmalloc-backed (linear memory grows on demand — no fixed
// capacity to size). Filter itself never allocates guest-side: matched
// records are copied straight into the host-provided out buffer.
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

#[unsafe(export_name = "run")]
// out_len_ptr: pointer to a host-allocated 4-byte length slot in guest
// linear memory. On success the written output length is stored there as
// BE u32 for the host to read
// (Spec.md §4: variable-length output; the host reads this length and then
// out_ptr[0..length]).
pub extern "C" fn run(
    input_ptr: u32,
    input_len: u32,
    params_ptr: u32,
    params_len: u32,
    out_ptr: u32,
    out_len_ptr: u32,
) -> i32 {
    unsafe {
        // ---- Parameter parsing (Spec.md §4, 28 bytes total) ----
        let mut param_cursor = Cursor::new(params_ptr, params_len);

        // version: u8 at offset 0. Mismatch is always rejected, never coerced.
        let version: u8 = match param_cursor.read_bytes(1) {
            Ok(v) => v[0],
            Err(()) => return E_INVALID_PARAM,
        };
        if version != 0x01 {
            return E_INVALID_PARAM;
        }
        // Exactly 27 bytes remain after the version byte; trailing bytes are
        // always rejected (Spec.md §0).
        let remaining = param_cursor.remaining();
        if remaining > 27 {
            return E_TRAILING_DATA;
        }
        if remaining < 27 {
            return E_INVALID_PARAM;
        }

        // record_offset at 1..5, field_offset at 5..9 (both u32 BE).
        let record_offset: u32 = match param_cursor.peek_field(0, 4, 0) {
            Ok(v) => v as u32,
            Err(()) => return E_INVALID_PARAM,
        };
        let field_offset: u32 = match param_cursor.peek_field(4, 4, 0) {
            Ok(v) => v as u32,
            Err(()) => return E_INVALID_PARAM,
        };
        if record_offset == 0 {
            // A zero-width record makes `input_len % record_offset` and the
            // record walk undefined; reject before any division.
            return E_INVALID_PARAM;
        }

        // Skip to offset 9 (version byte + record_offset + field_offset):
        // the cursor is at offset 1 after the version read, so advance 8.
        if param_cursor.advance(8).is_err() {
            return E_INVALID_PARAM;
        }
        let field_width: u8;
        match param_cursor.read_bytes(1) {
            Ok(v) => field_width = v[0],
            Err(()) => return E_INVALID_PARAM,
        }
        if field_width != 4 && field_width != 8 {
            return E_INVALID_PARAM;
        }
        let signed: u8;
        match param_cursor.read_bytes(1) {
            Ok(v) => signed = v[0],
            Err(()) => return E_INVALID_PARAM,
        }
        if signed > 1 {
            return E_INVALID_PARAM;
        }

        // predicate_type at 11 (cursor is now at offset 11).
        let predicate_type: u8;
        match param_cursor.read_bytes(1) {
            Ok(v) => predicate_type = v[0],
            Err(()) => return E_INVALID_PARAM,
        }
        if predicate_type > 2 {
            return E_INVALID_PARAM;
        }

        // operand_1 at 12..20, operand_2 at 20..28 (i64 BE), relative to the
        // cursor now at offset 12.
        let operand_1: i64;
        match param_cursor.peek_field(0, 8, 1) {
            Ok(v) => operand_1 = v,
            Err(()) => return E_INVALID_PARAM,
        }
        let operand_2: i64;
        match param_cursor.peek_field(8, 8, 1) {
            Ok(v) => operand_2 = v,
            Err(()) => return E_INVALID_PARAM,
        }

        // ---- Predicate validation (Spec.md §4) ----
        match predicate_type {
            0 => {
                // range: keep if operand_1 <= value < operand_2.
                if operand_2 <= operand_1 {
                    return E_INVALID_PARAM;
                }
            }
            _ => {
                // equals / bitmask: operand_2 must be exactly 0.
                if operand_2 != 0 {
                    return E_INVALID_PARAM;
                }
            }
        }

        // Spec.md §2: each record is record_offset bytes and holds the field at
        // field_offset..field_offset+field_width. If the field runs past the
        // record, extraction of the final record would read past the input
        // buffer and trap; reject such params instead.
        if (field_offset as u64) + (field_width as u64) > record_offset as u64 {
            return E_INVALID_PARAM;
        }

        // ---- Input shape (Spec.md §2) ----
        if input_len % record_offset != 0 {
            return E_MALFORMED_INPUT;
        }

        // Output region: worst case every record matches, so the host sizes
        // the out buffer at input_len. `written` never exceeds input_len.
        let out = core::slice::from_raw_parts_mut(out_ptr as *mut u8, input_len as usize);

        let mut input_cursor = Cursor::new(input_ptr, input_len);
        let mut written: u32 = 0;
        while !input_cursor.at_end() {
            let value = match input_cursor.peek_field(field_offset, field_width, signed) {
                Ok(v) => v,
                Err(()) => return E_MALFORMED_INPUT,
            };
            // Strictly sequential evaluation and emission in input order
            // (Spec.md §4 determinism note) — never reorder.
            let keep = match predicate_type {
                0 => operand_1 <= value && value < operand_2,
                1 => value == operand_1,
                _ => (value & operand_1) != 0,
            };

            // The whole record is emitted on a match, not just the field.
            let rec = match input_cursor.peek_bytes(record_offset) {
                Ok(s) => s,
                Err(()) => return E_MALFORMED_INPUT,
            };
            if keep {
                let start = written as usize;
                out[start..start + record_offset as usize].copy_from_slice(rec);
                written = written.wrapping_add(record_offset); // <= input_len, no wrap
            }

            if input_cursor.advance(record_offset).is_err() {
                return E_MALFORMED_INPUT;
            }
        }

        // Store the written output length (BE u32) in the host-provided slot.
        let len_out = core::slice::from_raw_parts_mut(out_len_ptr as *mut u8, 4);
        len_out.copy_from_slice(&written.to_be_bytes());

        return E_OK;
    }
}

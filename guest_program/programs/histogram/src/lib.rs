#![no_std]

use engram_shared::{error::*, heap, Cursor};

// The host stages params/input/out/length-slot through the exported `alloc`,
// which is dlmalloc-backed (linear memory grows on demand — no fixed
// capacity to size). The bin table below comes from the same heap.
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
// where variable-length outputs store their length (BE u32). Histogram's
// output size is param-deterministic, so the slot is accepted and ignored.
#[unsafe(export_name = "run")]
pub extern "C" fn run(
    input_ptr: u32,
    input_len: u32,
    params_ptr: u32,
    params_len: u32,
    out_ptr: u32,
    _out_len_ptr: u32,
) -> i32 {
    unsafe {
        // ---- Parameter parsing (Spec.md §5, 35 bytes total) ----
        let mut param_cursor = Cursor::new(params_ptr, params_len);

        // version: u8 at offset 0. Mismatch is always rejected, never coerced.
        let version: u8 = match param_cursor.read_bytes(1) {
            Ok(v) => v[0],
            Err(()) => return E_INVALID_PARAM,
        };
        if version != 0x01 {
            return E_INVALID_PARAM;
        }
        // Exactly 34 bytes remain after the version byte; trailing bytes are
        // always rejected (Spec.md §0).
        let remaining = param_cursor.remaining();
        if remaining > 34 {
            return E_TRAILING_DATA;
        }
        if remaining < 34 {
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

        // num_bins at 11..15, min_value at 15..23, max_value at 23..31,
        // flags at 31..35, all relative to the cursor at offset 11.
        let num_bins: u32;
        match param_cursor.peek_field(0, 4, 0) {
            Ok(v) => num_bins = v as u32,
            Err(()) => return E_INVALID_PARAM,
        }
        let min_value: i64;
        match param_cursor.peek_field(4, 8, 1) {
            Ok(v) => min_value = v,
            Err(()) => return E_INVALID_PARAM,
        }
        let max_value: i64;
        match param_cursor.peek_field(12, 8, 1) {
            Ok(v) => max_value = v,
            Err(()) => return E_INVALID_PARAM,
        }
        let flags: u32;
        match param_cursor.peek_field(20, 4, 0) {
            Ok(v) => flags = v as u32,
            Err(()) => return E_INVALID_PARAM,
        }

        // ---- Parameter validation (Spec.md §5) ----
        // num_bins must be in [1, 65535] — strictly smaller than 65536
        // (Spec.md §5 table; the Validation bullet's "> 65536" is a doc bug).
        if num_bins == 0 || num_bins >= 65536 {
            return E_INVALID_PARAM;
        }
        if max_value <= min_value {
            return E_INVALID_PARAM;
        }
        // Reserved flag bits 1..31 must be zero.
        if flags & 0xFFFF_FFFE != 0 {
            return E_UNKNOWN_FLAG;
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

        // Bin table on the dlmalloc heap (heap::alloc returns 8-byte aligned
        // memory, so no manual padding is needed for i64 access).
        let bins_ptr = heap::alloc(((num_bins as usize) * 8) as u32);
        let counts: &mut [i64] =
            core::slice::from_raw_parts_mut(bins_ptr as *mut i64, num_bins as usize);
        for c in counts.iter_mut() {
            *c = 0;
        }

        // ---- Bucketing (Spec.md §5) ----
        // bin_width = (max_value - min_value) / num_bins, integer division.
        // max > min was validated above; a true span >= 2^63 wraps negative
        // under i64 and cannot be represented, so reject rather than emit
        // garbage.
        let span = max_value.wrapping_sub(min_value);
        if span <= 0 {
            return E_INVALID_PARAM;
        }
        let bin_width = span.wrapping_div(num_bins as i64);
        if bin_width == 0 {
            // num_bins > (max_value - min_value): zero-width bins make the
            // spec's division formula undefined; reject instead of dividing
            // by zero inside the loop.
            return E_INVALID_PARAM;
        }

        let drop_out_of_range = (flags & 1) == 1; // bit 0: 1 = drop, 0 = clamp
        let bin_count_i64 = num_bins as i64;

        let mut input_cursor = Cursor::new(input_ptr, input_len);
        while !input_cursor.at_end() {
            let value = match input_cursor.peek_field(field_offset, field_width, signed) {
                Ok(v) => v,
                Err(()) => return E_MALFORMED_INPUT,
            };

            if drop_out_of_range {
                // Dropped if out of [min_value, max_value); values whose
                // computed index falls past the last bin (the range remainder
                // tail) likewise belong to no bin and are dropped.
                if value < min_value || value >= max_value {
                    // skip this record
                } else {
                    let index = value.wrapping_sub(min_value).wrapping_div(bin_width);
                    if index < bin_count_i64 {
                        counts[index as usize] = counts[index as usize].wrapping_add(1);
                    }
                }
            } else {
                // Clamp mode: out-of-range values and the range-remainder tail
                // land on the nearest edge bin, never past num_bins - 1.
                let index: usize = if value < min_value {
                    0
                } else if value >= max_value {
                    (num_bins - 1) as usize
                } else {
                    let raw = value.wrapping_sub(min_value).wrapping_div(bin_width);
                    if raw >= bin_count_i64 {
                        (num_bins - 1) as usize
                    } else {
                        raw as usize
                    }
                };
                counts[index] = counts[index].wrapping_add(1);
            }

            if input_cursor.advance(record_offset).is_err() {
                return E_MALFORMED_INPUT;
            }
        }

        // ---- Output (Spec.md §5) ----
        // For i in 0..num_bins ascending: bin_index: u32 BE, count: i64 BE.
        // 12 bytes per bin; zero-count bins are always emitted.
        let out_len = (num_bins as usize) * 12;
        let out = core::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len);
        for i in 0..num_bins as usize {
            let base = i * 12;
            out[base..base + 4].copy_from_slice(&(i as u32).to_be_bytes());
            out[base + 4..base + 12].copy_from_slice(&counts[i].to_be_bytes());
        }

        return E_OK;
    }
}

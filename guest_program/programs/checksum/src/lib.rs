#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use ark_bn254::Fr;
use ark_ff::{Field, PrimeField, Zero, One};
use engram_shared::{error::*, heap, Cursor};

// The host stages params/input/output through the exported `alloc`, which is
// dlmalloc-backed (linear memory grows on demand — no fixed capacity to
// size). The Merkle builder will allocate its per-level hash buffers from
// the same heap once the Poseidon backend is implemented.
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

fn pkcs7_pad(data: &[u8]) -> [u8; 31] {
    let pad_len = (31 - data.len()) as u8; // always in 1..=31
    let mut out = [0u8; 31];
    out[..data.len()].copy_from_slice(data);
    out[data.len()..].fill(pad_len);
    return out;
}
extern "C" fn poseidon_one_pass(
    data_ptr: u32,
    data_len: u32
) -> Result<(), ()> {
    
    let mut padding: bool = false;
    if data_len > 93 {
        return Err(());
    }
    else if data_len < 93 {
        padding = true;
    }
    else {
        padding = false;
    }
    
    let safe_value = (data_len / 31) as usize;
    let num_of_leftover = (data_len % 31) as usize;
    let mut internal_state = [Zero::zero(); 4];
 
    unsafe{
        for index in 0..safe_value {
            let val_start_ptr = (data_ptr + (index * 31) as u32) as *const u8;
            let chunk = core::slice::from_raw_parts(val_start_ptr, 31);
            let elt = Fr::from_be_bytes_mod_order(chunk);
            internal_state[index] = elt;
        }
        if padding {
            let start_ptr = (data_ptr + (safe_value * 31) as u32) as *const u8;
            let leftover = core::slice::from_raw_parts(start_ptr, num_of_leftover);
            internal_state[safe_value] = Fr::from_be_bytes_mod_order(&pkcs7_pad(leftover));
        }
    }


    return Ok(());
}


#[unsafe(export_name = "run")]
// _input_ptr/_input_len/_out_ptr/_out_len_ptr: the uniform 6-arg ABI is kept
// for host dispatch, but only the params are consumed by this stage.
pub extern "C" fn run(
    _input_ptr: u32,
    _input_len: u32,
    params_ptr: u32,
    params_len: u32,
    _out_ptr: u32,
    _out_len_ptr: u32,
) -> i32 {
    // ---- Parameter parsing (Spec.md §6, 5 bytes total) ----
    let mut param_cursor = Cursor::new(params_ptr, params_len);

    // version: u8 at offset 0. Mismatch is always rejected, never coerced.
    let version: u8 = match param_cursor.read_bytes(1) {
        Ok(v) => v[0],
        Err(()) => return E_INVALID_PARAM,
    };
    if version != 0x01 {
        return E_INVALID_PARAM;
    }
    // Exactly 1 bytes remain after the version byte; trailing bytes are
    // always rejected (Spec.md §0).
    let remaining = param_cursor.remaining();
    if remaining > 1 {
        return E_TRAILING_DATA;
    }
    if remaining < 1 {
        return E_INVALID_PARAM;
    }

    // Do we have to create a Merkle tree, or do we just have to create
    // a checksum for the input blob? 1 byte, 0 or 1.
    let is_merkle: u8 = match param_cursor.read_bytes(1) {
        Ok(v) => v[0],
        Err(()) => return E_INVALID_PARAM,
    };
    if is_merkle != 0 && is_merkle != 1 {
        return E_INVALID_PARAM;
    }

    // ---- Digest / Merkleization (Spec.md §6, steps 1-3) ----
    
    return E_OK;
}

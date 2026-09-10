#![no_std]

//! Shared guest runtime: error codes, the field-extraction contract, a
//! bounds-checked cursor, and the dlmalloc-backed heap that every guest
//! program allocates from.

extern crate alloc;

pub mod error {
    pub const E_OK: i32 = 0;
    pub const E_FUEL_EXHAUSTED: i32 = 1;
    pub const E_INVALID_PARAM: i32 = 2;
    pub const E_UNKNOWN_FLAG: i32 = 3;
    pub const E_TRAILING_DATA: i32 = 4;
    pub const E_MALFORMED_INPUT: i32 = 5;
    pub const E_BUF_OVERFLOW: i32 = 6;
}

/// Guest-side heap: `dlmalloc::GlobalDlmalloc` is the single global
/// allocator. It grows the wasm linear memory on demand, so there is no
/// fixed capacity to size by hand anymore — the previous `Arena<N>` bump
/// allocator has been removed.
pub mod heap {
    use core::alloc::Layout;
    use dlmalloc::GlobalDlmalloc;

    #[global_allocator]
    static ALLOCATOR: GlobalDlmalloc = GlobalDlmalloc;

    /// Every guest allocation is 8-byte aligned — enough for `u64`/`i64`
    /// slices (guest logic has no floating point, Spec.md §0).
    const ALIGN: usize = 8;

    /// Non-null, 8-aligned sentinel returned for zero-sized requests.
    /// Rust's `alloc` requires a non-zero layout, and callers write nothing
    /// for a zero-length region.
    #[repr(align(8))]
    struct ZeroSized([u8; 8]);
    static ZERO_SIZED: ZeroSized = ZeroSized([0; 8]);

    /// Allocate `size` bytes and return a guest linear-memory address.
    ///
    /// Zero-sized requests get a non-null sentinel. A failed allocation
    /// traps (`unreachable`) instead of returning null: handing the host a
    /// null pointer would silently corrupt low guest memory.
    pub fn alloc(size: u32) -> u32 {
        if size == 0 {
            return (&raw const ZERO_SIZED).cast::<u8>() as usize as u32;
        }
        let layout = match Layout::from_size_align(size as usize, ALIGN) {
            Ok(layout) => layout,
            Err(_) => core::arch::wasm32::unreachable(),
        };
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            core::arch::wasm32::unreachable()
        } else {
            ptr as usize as u32
        }
    }

    /// Free a block previously returned by [`alloc`] with the same `size`.
    /// (Nothing in the current guests calls this — an instance is created
    /// per run — but the export exists so a host that reuses an instance can
    /// release staging buffers.)
    pub fn dealloc(ptr: u32, size: u32) {
        if size == 0 {
            return;
        }
        let layout = match Layout::from_size_align(size as usize, ALIGN) {
            Ok(layout) => layout,
            Err(_) => return,
        };
        unsafe { alloc::alloc::dealloc(ptr as *mut u8, layout) }
    }
}

pub fn extract_field(
    record_ptr: u32,
    field_offset: u32,
    field_width: u8,
    is_signed: u8,
) -> Result<i64, ()> {
    unsafe {
        let addr = (record_ptr + field_offset) as *const u8;
        match field_width {
            4 => {
                let bytes: [u8; 4] = core::slice::from_raw_parts(addr, 4).try_into().unwrap();
                Ok(if is_signed == 1 {
                    i32::from_be_bytes(bytes) as i64
                } else {
                    u32::from_be_bytes(bytes) as i64
                })
            }
            8 => {
                let bytes: [u8; 8] = core::slice::from_raw_parts(addr, 8).try_into().unwrap();
                Ok(i64::from_be_bytes(bytes)) // always signed per spec
            }
            _ => Err(()),
        }
    }
}

pub struct Cursor {
    base: u32,   // start of the buffer in guest linear memory
    len: u32,    // total valid length of the buffer
    pos: u32,    // current read position, relative to `base`
}

impl Cursor {
    pub fn new(base: u32, len: u32) -> Self {
        Cursor { base, len, pos: 0 }
    }

    pub fn remaining(&self) -> u32 {
        self.len - self.pos   // pos never exceeds len, invariant maintained below
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.len
    }

    /// Returns the absolute guest-memory address of the current position,
    /// after checking there's room to read `n` bytes here.
    /// This is the ONLY place bounds-checking happens for reads.
    fn checked_addr(&self, n: u32) -> Result<u32, ()> {
        let end = self.pos.checked_add(n).ok_or(())?;
        if end > self.len {
            return Err(());  // would read past the buffer — reject, don't wrap
        }
        self.base.checked_add(self.pos).ok_or(())
    }

    /// Reads `n` bytes at the current position WITHOUT advancing.
    pub fn peek_bytes(&self, n: u32) -> Result<&[u8], ()> {
        let addr = self.checked_addr(n)?;
        unsafe {
            Ok(core::slice::from_raw_parts(addr as *const u8, n as usize))
        }
    }

    /// Reads `n` bytes and advances the cursor past them.
    pub fn read_bytes(&mut self, n: u32) -> Result<&[u8], ()> {
        let addr = self.checked_addr(n)?;   // <- returns an owned u32, borrow of self ends immediately here
        self.pos = self.pos.checked_add(n).ok_or(())?;  // no live borrow anymore — mutation is fine
        unsafe {
            Ok(core::slice::from_raw_parts(addr as *const u8, n as usize))
        }
    }

    /// Advances the cursor by `n` bytes without reading — used to skip
    /// past the unused portion of a record (e.g. the ID/flag bytes
    /// around your extracted field).
    pub fn advance(&mut self, n: u32) -> Result<(), ()> {
        let new_pos = self.pos.checked_add(n).ok_or(())?;
        if new_pos > self.len {
            return Err(());
        }
        self.pos = new_pos;
        Ok(())
    }

    /// Convenience: read a field using the existing extract_field logic,
    /// at an offset relative to the CURRENT position (i.e. within the
    /// record that starts here), without advancing.
    pub fn peek_field(&self, field_offset: u32, field_width: u8, is_signed: u8) -> Result<i64, ()> {
        let record_addr = self.checked_addr(0)?; // just validates pos is in-bounds
        extract_field(record_addr, field_offset, field_width, is_signed)
    }
}
/// One panic handler, shared by every program that depends on this
/// crate — each program's compiled .wasm gets its own copy at link
/// time, so there's no "duplicate lang item" conflict across binaries.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

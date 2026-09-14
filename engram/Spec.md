# Engram-Transform Phase 0: Determinism Specification

## 0. Global Conventions

These rules apply to every program, every encoding, without exception.

- **Byte order**: big-endian for all multi-byte integers.
- **Integer types**: fixed-width only (`u8`, `u32`, `u64`, `i32`, `i64`). No variable-length varints.
- **No implicit padding/alignment**: fields are packed back-to-back in the exact byte order specified. No struct-alignment padding.
- **Field order**: fixed and explicit, as defined per program below. Never derived from map/dict iteration order.
- **Arithmetic**: integer/fixed-point only. No floating point anywhere in guest logic.
- **Overflow**: defined wraparound (two's-complement), not a trap or undefined behaviors.
    - Rust: use `wrapping_add`/`wrapping_sub`/etc.
- **Every parameter blob starts with a `version: u8` byte.** Currently `0x01` for all programs. A mismatched version is always rejected, never coerced.
- **Trailing bytes are always rejected.** If a parameter or input blob has bytes beyond what the spec defines, return `E_TRAILING_DATA`. Never silently ignore extra bytes.
- **Guest execution environment**: compiled to `wasm32` with no ambient capabilities (no filesystem, clock, network, randomness). Rust: `wasm32-unknown-unknown`.
- **Sandbox**: Wasmtime with fuel metering enabled, fixed fuel budget per run. Fuel exhaustion always returns the same error code.

## 1. Shared Error Code Table

| Code | Name                | Meaning                                                        |
| ---- | ------------------- | -------------------------------------------------------------- |
| `0`  | `E_OK`              | Success                                                        |
| `1`  | `E_FUEL_EXHAUSTED`  | Fuel budget exceeded (raised by host/runtime, not guest logic) |
| `2`  | `E_INVALID_PARAM`   | Parameter value out of allowed range or malformed              |
| `3`  | `E_UNKNOWN_FLAG`    | Reserved flag/field bits set                                   |
| `4`  | `E_TRAILING_DATA`   | Blob longer than its defined schema                            |
| `5`  | `E_MALFORMED_INPUT` | Input length not consistent with record width / chunk size     |
| `6`  | `E_BUF_OVERFLOW`    | Pointer points to somewhere outside the allocated memory.      |

Every program below uses only this table — no per-program error codes.

## 2. Shared Field-Extraction Contract

`filter`, `count/sum`, and `histogram` all consume input as a sequence of fixed-width records and extract one field per record. This is implemented once, reused by all three.

**Input encoding (shared)**:

```
Input = sequence of records, each record_width bytes
record_width = field_offset + field_width
```

- `input_len % record_width != 0` → `E_MALFORMED_INPUT`

**`extract_field(record, field_offset, field_width, is_signed) -> i64`**:

- Reads `field_width` bytes starting at `field_offset`, big-endian.
- `field_width == 4`: sign-extend if `is_signed == 1`, else zero-extend.
- `field_width == 8`: always treated as signed `i64`.
- `field_width` not in `{4, 8}` → `E_INVALID_PARAM`.

## 3. `count / sum`

**Parameters (11 bytes):**

| Offset | Size | Field           | Notes      |
| ------ | ---- | --------------- | ---------- |
| 0      | 1    | `version`       | `0x01`     |
| 1      | 4    | `record_offset` | `u32` BE   |
| 5      | 4    | `field_offset`  | `u32` BE   |
| 9      | 1    | `field_width`   | `4` or `8` |
| 10     | 1    | `signed`        | `0`/`1`    |

**Output (8 bytes)**: `sum: i64` BE, wrapping arithmetic.

**Logic**: `sum = Σ extract_field(record_i)` over all records, in input order.

**Worked example** — record 32, offset 0, width 8, signed:

```
01                       version
00 00 01 00              record_offset = 32
00 00 00 00              field_offset = 0
08                       field_width = 8
01                       signed = 1
```

## 4. `filter`

**Parameters (28 bytes):**

| Offset | Size | Field            | Notes                              |
| ------ | ---- | ---------------- | ---------------------------------- |
| 0      | 1    | `version`        | `0x01`                             |
| 1      | 4    | `record_offset`  | `u32` BE                           |
| 5      | 4    | `field_offset`   | `u32` BE                           |
| 9      | 1    | `field_width`    | `4` or `8`                         |
| 10     | 1    | `signed`         | `0`/`1`                            |
| 11     | 1    | `predicate_type` | `0`=range, `1`=equals, `2`=bitmask |
| 12     | 8    | `operand_1`      | `i64` BE                           |
| 20     | 8    | `operand_2`      | `i64` BE, unused unless range      |

**Predicate semantics:**

- `predicate_type == 0` (range): keep if `operand_1 <= value < operand_2`. `operand_2 <= operand_1` → `E_INVALID_PARAM`.
- `predicate_type == 1` (equals): keep if `value == operand_1`. `operand_2 != 0` → `E_INVALID_PARAM`.
- `predicate_type == 2` (bitmask): keep if `(value & operand_1) != 0`. `operand_2 != 0` → `E_INVALID_PARAM`.
- `predicate_type > 2` → `E_INVALID_PARAM`.

**Output**: concatenation of matching records, `record_width` bytes each, **in original input order**, no separator, no count prefix. Empty output (zero matches) is valid and hashes as `H("")`.

**Determinism note**: evaluation and emission must be strictly sequential in input order. Never parallelize in a way that can reorder output.

## 5. `histogram`

**Parameters (35 bytes):**

| Offset | Size | Field           | Notes                                                                       |
| ------ | ---- | --------------- | --------------------------------------------------------------------------- |
| 0      | 1    | `version`       | `0x01`                                                                      |
| 1      | 4    | `record_offset` | `u32` BE                                                                    |
| 5      | 4    | `field_offset`  | `u32` BE                                                                    |
| 9      | 1    | `field_width`   | `u8`, `4` or `8`                                                            |
| 10     | 1    | `signed`        | `u8`, `0` or `1`                                                            |
| 11     | 4    | `num_bins`      | `u32` BE, must be > 0 and no more than 65535.                               |
| 15     | 8    | `min_value`     | `i64` BE                                                                    |
| 23     | 8    | `max_value`     | `i64` BE, must be > `min_value`                                             |
| 31     | 4    | `flags`         | bit 0: `1` = drop out-of-range, `0` = clamp; bits 1–31 reserved (must be 0) |

**Validation**: `num_bins == 0` or `num_bins > 65536` → `E_INVALID_PARAM`. `max_value <= min_value` → `E_INVALID_PARAM`. Reserved flag bits set → `E_UNKNOWN_FLAG`.

**Bucketing**: `bin_width = (max_value - min_value) / num_bins`; `bin_index = (value - min_value) / bin_width`, clamped to `[0, num_bins - 1]` if `flags` bit 0 is `0`; dropped if bit 0 is `1` and out of `[min_value, max_value)`.

**Output**: for `i` in `0..num_bins` (ascending, no skipping): emit `bin_index: u32` BE, `count: i64` BE. Zero-count bins are still emitted — never omit a bin.

## 6. `checksum / Merkleization`

**Parameters (2 bytes):**

| Offset | Size | Field       | Notes                                     |
| ------ | ---- | ----------- | ----------------------------------------- |
| 0      | 1    | `version`   | `0x01`                                    |
| 1      | 1    | `is_merkle` | `u8`, `0` = checksum, `1` = Merkleization |

**Input**: raw bytes.

**Logic**:

1. Divide the bytes into chunks of 31-byte length, and convert those chunks into elements of the scalar field $F_{p}$, big-endian. If any chunk is shorter than 31 bytes, padding using PCKS#7 padding scheme.
2. If `is_merkle = 0`, construct a Poseidon sponge hash, with states being input being 1 chunk.
3. If `is_merkle = 1`, each 2 consecutive chunks is a leaf of the Merkle tree. If a leaf does not have enough chunk, use 0-filled chunks. If there is a single node in any level, bump it to the next level on the tree (i.e. the parent of that node contain the value of itself.)

**Output**: `root: 32 bytes` represent either a checksum of the blob, or the root of the Merkle tree.

**Validation**: `is_merkle != 0 or is_merkle != 1` -> `E_INVALID_PARAM`

**Hash function details**: POSEIDON, per Grassi et al., 2019.

| Parameter  | Choice                                               |
| ---------- | ---------------------------------------------------- |
| Field      | BN254 scalar field $F_{p}$ where $p \approx 2^{254}$ |
| S-box      | $SB(x) = x^5$                                        |
| Width $t$  | 3 field elements                                     |
| Capacity   | 1 field element                                      |
| Rounds     | $R_{F} = 8$, $R_{P}=56$                              |
| Sec. level | 128-bit                                              |

## 7. Datasets

| Dataset                   | Format                                                                                                 | Used by                                    |
| ------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------ |
| Synthetic integer records | Fixed-width records, stored as Parquet on disk; host extracts raw column bytes before passing to guest | `count`, `filter`, `histogram`, `checksum` |
| Structured logs           | Variable-length delimited records, Parquet or equivalent                                               | `count`, `filter`, `histogram`, `checksum` |
| Chunked binary records    | Raw bytes, pre-sliced by byte offset, no record structure                                              | `checksum` only                            |

Host responsibility: read the on-disk format (Parquet/Arrow), extract raw bytes, pass them to the guest. **The guest never touches Arrow/Parquet libraries directly** — only your own field-extraction/checksum code runs inside the sandbox.

## 8. Acceptance Gate (Phase 0 done when all of this holds)

1. For every program × supported dataset combination: running the same program, same input, same parameters 100 times produces an identical output digest every time.
2. A fuel-exceeding run fails with `E_FUEL_EXHAUSTED` identically every time.
3. Every validation rule above has a corresponding negative test (malformed params, trailing data, empty output, etc.).
4. A benchmark report exists covering timing/resource usage per program × dataset, no proofs involved.
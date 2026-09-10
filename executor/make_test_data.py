"""Generate Engram-Transform Phase 0 test data (Spec.md §7) as pickle-wrapped
byte blobs for the host's default ``pickle-bytes`` input format.

The host DOES NOT read Parquet: ``InputFormat`` -> ``pickle-bytes`` unpickles a
single ``bytes`` blob that becomes the guest's input verbatim. So each dataset
here is written as ``pickle.dumps(<raw bytes>)``.

Datasets (Spec.md §7):
  * synthetic integer records  -> fixed-width records (8-byte BE i64)   [count, filter, histogram, checksum]
  * structured logs            -> variable-length newline-delimited     [checksum]
  * chunked binary records     -> raw bytes, pre-sliced by byte offset  [checksum]

Also writes the matching raw param blobs (Spec.md §3/§4/§5/§6) so each flow is
runnable end-to-end:  executor <prog> --input data/<in>.pkl --params data/<prog>.params

All generation is deterministic (fixed seed / fixed literals) so the output is
byte-exact reproducible across runs, per Phase 0 Acceptance Gate §8.1.
"""

from __future__ import annotations

import pickle
import struct
from pathlib import Path

DATA_DIR = Path(__file__).resolve().parent / "src" / "data"

# param blobs are raw spec bytes, NOT pickled (the --params arg is a raw blob)

# count/sum (Spec §3): version, record_offset u32, field_offset u32,
#   field_width u8, signed u8                       -> 11 bytes
COUNT_PARAMS = bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1])
# filter (Spec §4): version, rec_off u32, field_off u32, field_width, signed,
#   predicate_type, operand_1 i64, operand_2 i64     -> 28 bytes
#   keep: range [0, 2^62) in signed i64 field at offset 0
FILTER_PARAMS = (
    bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1, 0])
    + struct.pack(">qq", 0, 1 << 62)
)
# histogram (Spec §5): version, rec_off u32, field_off u32, field_width, signed,
#   num_bins u32, min i64, max i64, flags u32        -> 35 bytes
#   8 bins over [0, 1024), clamp (flags 0)
HIST_PARAMS = (
    bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1])
    + struct.pack(">I", 8) + struct.pack(">qq", 0, 1024) + struct.pack(">I", 0)
)
# checksum (Spec §6): version, is_merkle u8          -> 2 bytes
CHECKSUM_PARAMS = bytes([0x01, 0x00])  # is_merkle = 0 (plain checksum)


def synthetic_integer_records(n: int = 256, record_bytes: int = 8) -> bytes:
    """§7 'synthetic integer records': fixed-width 8-byte BE i64 records.

    Deterministic spread of values: some negative, some positive, some zero,
    so count/filter/histogram have non-trivial work.
    """
    recs = []
    for i in range(n):
        # a spread of signed values incl. negatives and one zero-derived case
        v = (i * 641) % 2049 - 1024 if i % 7 else 0
        recs.append(struct.pack(">q", v))
    assert all(len(r) == record_bytes for r in recs)
    return b"".join(recs)


def structured_logs(n: int = 64) -> bytes:
    """§7 'structured logs': variable-length newline-delimited records.

    Deterministic (fixed timestamps, no clock): lines hold tab-joined fields.
    """
    lines = []
    for i in range(n):
        # fixed synthetic timestamp + level + payload — never calls time()
        line = f"ts=1700000000+{i}\tlevel=INFO\tmsg=record_{i}\n"
        lines.append(line.encode())
    return b"".join(lines)


def chunked_binary(size: int = 4096, seed: int = 42) -> bytes:
    """§7 'chunked binary records': raw bytes, no record structure (checksum).

    Deterministic pseudo-random bytes (fixed LCG, no RNG module state) sized
    to exercise >1 Poseidon chunk (31-byte chunks) once checksum is built.
    """
    out = bytearray()
    x = seed
    while len(out) < size:
        x = (x * 1103515245 + 12345) % (1 << 31)  # deterministic LCG
        out.append((x >> 16) & 0xFF)
    return bytes(out)


def write_blob(p: Path, data: bytes) -> None:
    p.write_bytes(pickle.dumps(data))


def main() -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)  # (1) create data/ if absent

    print(f"data dir: {DATA_DIR}")

    # (2) + (3) generate and write each §7 dataset as a pickle-wrapped blob
    synthetic = synthetic_integer_records()
    write_blob(DATA_DIR / "synthetic_integer_records.pkl", synthetic)

    logs = structured_logs()
    write_blob(DATA_DIR / "structured_logs.pkl", logs)

    binary = chunked_binary()
    write_blob(DATA_DIR / "chunked_binary.pkl", binary)

    # raw param blobs per program (NOT pickled)
    params = {
        "count": COUNT_PARAMS,
        "filter": FILTER_PARAMS,
        "histogram": HIST_PARAMS,
        "checksum": CHECKSUM_PARAMS,
    }
    for name, p_blob in params.items():
        (DATA_DIR / f"{name}.params").write_bytes(p_blob)

    # a small manifest so a reader knows what each file is
    manifest = DATA_DIR / "README.md"
    manifest.write_text(
        "# Engram-Transform test data (Spec.md §7)\n"
        "\n"
        "Inputs are pickle-wrapped raw bytes for the host's `pickle-bytes`\n"
        "format; params are raw spec blobs (not pickled).\n"
        "\n"
        "| file | dataset | programs |\n"
        "| --- | --- | --- |\n"
        f"| synthetic_integer_records.pkl | synthetic integer records ({len(synthetic)} B) | count, filter, histogram, checksum |\n"
        f"| structured_logs.pkl | structured logs ({len(logs)} B) | checksum |\n"
        f"| chunked_binary.pkl | chunked binary records ({len(binary)} B) | checksum |\n"
        f"| count.params | count params ({len(COUNT_PARAMS)} B) | count |\n"
        f"| filter.params | filter params ({len(FILTER_PARAMS)} B) | filter |\n"
        f"| histogram.params | histogram params ({len(HIST_PARAMS)} B) | histogram |\n"
        f"| checksum.params | checksum params ({len(CHECKSUM_PARAMS)} B) | checksum |\n"
        "\n"
        "Example:  executor count --input src/data/synthetic_integer_records.pkl"
        " --params src/data/count.params\n"
    )

    for path in sorted(DATA_DIR.iterdir()):
        print(f"  {path.name}  ({path.stat().st_size} B)")


if __name__ == "__main__":
    main()
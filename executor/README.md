# executor

Sandboxed host for the Engram-Transform Phase 0 guest programs (Spec.md).
Written against `wasmtime` 48 (installed in `.venv`).

## Usage

```sh
uv run executor <program> --input <file> --params <params.bin> \
                [--input-format NAME] [--output-format NAME] \
                [--out-dir DIR] [--fuel N] [--wasm-dir DIR]
uv run executor --list-formats   # show available formats
# or, without uv:
.venv/bin/executor count --input in.pkl --params count.params
```

* `<program>` — `count` | `filter` | `histogram` (`checksum` is rejected until
  its guest is implemented).
* `--input` — input file, decoded by `--input-format`.
* `--input-format` — default `pickle-bytes`: the file is a **pickle** holding
  one raw `bytes` blob, passed to the guest verbatim. ⚠️ `pickle` runs code on
  load: only load files you created. Also built-in: `raw-bytes` (file bytes
  used verbatim).
* `--params` — the **raw spec param blob** (version byte `0x01` first), exactly
  as Spec.md defines per program.
* `--output-format` — default `raw`: writes `<out-dir>/output.bin`.
* `--out-dir` — where the output format writes (default `.`).
* `--fuel` — deterministic per-run fuel budget (default `1000000000`). Lower it
  to exercise `E_FUEL_EXHAUSTED` deterministically.
* `--wasm-dir` — guest wasm directory or a direct `.wasm` path. Default lookup:
  `guest_program/target/wasm32-unknown-unknown/{debug,release}/<program>.wasm`
  (debug first — plain `cargo build` in `guest_program/` refreshes it).

The wasm guest is loaded from `guest_program/target/...`; build it with
`cargo build -p count` (etc.) from `../guest_program`.

### Exit codes

| code | meaning |
| --- | --- |
| 0 | `E_OK` — result written by the output format |
| 1–6 | guest returned `E_FUEL_EXHAUSTED` … `E_BUF_OVERFLOW` (Spec.md §1) |
| 64 | host-side error (bad file, sandbox breach, format error, …) |
| 2 | CLI usage error |

## Host guarantees (Spec.md §0)

* Deterministic fuel: `Config.consume_fuel = True` + `store.set_fuel(fuel)`.
* No ambient capabilities: SIMD/threads/shared-memory/memory64/GC/component
  model/exceptions/tail-call/etc. disabled; **no WASI and no host imports are
  ever provided** — instantiation fails closed if the module imports anything.
* One fresh `Store` per run — no state, heap, or fuel carried between jobs.

## Pluggable I/O formats

`src/executor/engine.py` never touches files except reading the params blob;
input decoding and result writing live behind two base classes in
`src/executor/formats/base.py`:

* `InputFormat` — `load(self, path, *, program, params) -> bytes` returns the
  exact input blob for the guest (context is passed so a format can encode
  records from params if it needs to).
* `OutputFormat` — `write(self, result: RunResult, out_dir) -> Path`
  materializes a successful run (`RunResult` = program + params + raw output
  bytes) onto disk.

**To add a format, write a child class in any module under `executor/formats/`
— it registers itself automatically when the package loads.** Example:

```python
# executor/formats/json_output.py
import json
from pathlib import Path
from .base import OutputFormat, RunResult

class JsonOutput(OutputFormat):
    name = "json"

    def write(self, result: RunResult, out_dir: Path) -> Path:
        out_dir.mkdir(parents=True, exist_ok=True)
        out_path = out_dir / "result.json"
        out_path.write_text(json.dumps({
            "program": result.program,
            "output_hex": result.output.hex(),
        }))
        return out_path
```

Then: `uv run executor count --input in.pkl --params count.params --output-format json`

## Making test files

```python
import pickle, struct

# count/sum params (11 B): version, record_offset u32, field_offset u32,
# field_width, signed  (Spec.md §3)
count_params = bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1])
open("count.params", "wb").write(count_params)

# input: one pickled bytes blob — e.g. 4 records of 8-byte BE i64 values
records = b"".join(struct.pack(">q", v) for v in [1, 2, 3, 4])
open("in.pkl", "wb").write(pickle.dumps(records))

# filter params (28 B): §4 — keep values in [10, 20)
filter_params = (bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1, 0])
                 + struct.pack(">qq", 10, 20))
# histogram params (35 B): §5 — 4 bins over [0, 100), clamp (flags=0)
hist_params = (bytes([0x01]) + struct.pack(">II", 8, 0) + bytes([8, 1])
               + struct.pack(">I", 4) + struct.pack(">qq", 0, 100)
               + struct.pack(">I", 0))
```

## Capacity notes (guest heap grows on demand)

The host stages params + input + output + a 4-byte length slot in guest
linear memory. The guest allocates from `dlmalloc` (see `guest_program/shared`),
which grows the wasm memory on demand, so there is no fixed capacity to size
by hand: filter copies matched records straight into the host-provided out
region, and histogram allocates its bin table on the heap inside `run`. Only
a **failed** allocation traps (`unreachable`) — e.g. a truly huge input that
exhausts the process memory — and the executor prints a hint.

## ABI (what the host calls)

`alloc(size: u32) -> u32` (dlmalloc-backed heap), then
`run(input_ptr, input_len, params_ptr, params_len, out_ptr, out_len_ptr) -> i32`.
Fixed-size outputs (count 8 B, histogram 12×num_bins) are sized by the host
from params; filter stores its written length (BE u32) at `out_len_ptr` — the
host allocates that 4-byte slot and reads it back after `E_OK`.

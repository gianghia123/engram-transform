"""Sandboxed executor for Engram-Transform Phase 0 guest programs.

Responsibilities (host side of the Spec.md guest contract):
  1. load the guest .wasm matching the requested program,
  2. configure Wasmtime deterministically and with no ambient capabilities
     (fuel metering, no SIMD/threads/GC/etc., no WASI, no host imports —
     instantiation fails closed if the module declares *any* import),
  3. stage the raw input blob (decoded by the selected InputFormat) and the
     parameters (raw spec blob, version byte first) into guest linear memory
     via the guest ``alloc``,
  4. call ``run(input_ptr, input_len, params_ptr, params_len, out_ptr,
     out_len_ptr)`` and hand the raw output back to the caller as a
     ``RunResult``. Writing to disk is the job of an OutputFormat (see
     ``executor/formats/``), not this module.

Output length handling (uniform guest ABI, agreed 2026-09):
  * count      -> fixed 8 bytes
  * histogram  -> num_bins * 12 (num_bins parsed from params, BE, offset 11)
  * checksum   -> fixed 32 bytes (guest not implemented yet; CLI blocks it)
  * filter     -> variable: the guest stores the written length (BE u32) in
                  the 4-byte length slot the host allocates in guest linear
                  memory and passes as out_len_ptr; the host reads it back
                  after a successful run.
"""

from __future__ import annotations

from pathlib import Path

import wasmtime as wasm

from .formats import InputFormat, RunResult

__all__ = ["ExecutorError", "GuestError", "execute", "PARAM_SIZES", "ERROR_NAMES"]


class ExecutorError(Exception):
    """Host-side failure: bad files, unsupported program, sandbox breach."""


class GuestError(ExecutorError):
    """The guest returned a non-E_OK code (or trapped) — carries the Spec.md code."""

    def __init__(self, code: int, detail: str) -> None:
        super().__init__(detail)
        self.code = code


# Spec.md §1 shared error code table.
ERROR_NAMES = {
    0: "E_OK",
    1: "E_FUEL_EXHAUSTED",
    2: "E_INVALID_PARAM",
    3: "E_UNKNOWN_FLAG",
    4: "E_TRAILING_DATA",
    5: "E_MALFORMED_INPUT",
    6: "E_BUF_OVERFLOW",
}

# Spec.md param blob sizes per program.
PARAM_SIZES = {"count": 11, "filter": 28, "histogram": 35, "checksum": 5}

# Program name -> output capacity in bytes (param-deterministic programs only;
# filter's capacity is its input length — worst case every record matches).
_OUTPUT_CAPACITY = {"count": 8, "histogram": None, "checksum": 32}
_FILTER = "filter"

# Deterministic fuel budget per run (Spec.md §0: fixed fuel budget, always
# the same E_FUEL_EXHAUSTED behavior). Lower it with --fuel to exercise
# exhaustion deterministically.
DEFAULT_FUEL = 1_000_000_000

# Histogram num_bins is BE u32 at param offset 11; spec max is 65535.
_HIST_NUM_BINS_OFFSET = 11
_HIST_BIN_ENTRY = 12  # u32 index + i64 count
_HIST_MAX_BINS = 65535

_REPO_ROOT = Path(__file__).resolve().parents[3]
_GUEST_TARGET = _REPO_ROOT / "guest_program" / "target" / "wasm32-unknown-unknown"
_DEFAULT_WASM_DIRS = (_GUEST_TARGET / "debug", _GUEST_TARGET / "release")


def load_params_bytes(path: Path) -> bytes:
    """The params file is the raw spec blob (version byte 0x01 first)."""
    try:
        return path.read_bytes()
    except OSError as exc:
        raise ExecutorError(f"cannot read params {path}: {exc}") from exc


def find_wasm(program: str, wasm_dir: Path | None = None) -> Path:
    """Locate <program>.wasm. Auto-searches debug then release under
    guest_program/target/wasm32-unknown-unknown, or uses --wasm-dir
    (a directory, or a direct path to a .wasm file)."""
    if wasm_dir is not None:
        if wasm_dir.is_file() and wasm_dir.suffix == ".wasm":
            return wasm_dir
        candidate = wasm_dir / f"{program}.wasm"
        if candidate.is_file():
            return candidate
        raise ExecutorError(f"no wasm at {candidate}")
    tried = []
    for d in _DEFAULT_WASM_DIRS:
        candidate = d / f"{program}.wasm"
        tried.append(candidate)
        if candidate.is_file():
            return candidate
    raise ExecutorError(
        f"no prebuilt wasm for '{program}'; tried: "
        + ", ".join(str(t) for t in tried)
        + ". Build with `cargo build -p <program>` in guest_program/ "
        "or pass --wasm-dir."
    )


def _out_capacity(program: str, params: bytes, input_len: int) -> int:
    """Bytes to allocate for the out region. Deterministic per program;
    filter gets input_len (worst case: every record matches)."""
    if program == _FILTER:
        return input_len
    if program == "histogram":
        if len(params) == PARAM_SIZES["histogram"] and params[0] == 0x01:
            num_bins = int.from_bytes(
                params[_HIST_NUM_BINS_OFFSET : _HIST_NUM_BINS_OFFSET + 4], "big"
            )
            if 1 <= num_bins <= _HIST_MAX_BINS:
                return num_bins * _HIST_BIN_ENTRY
        # Malformed params: don't allocate on a guess — let the guest reject.
        return 0
    cap = _OUTPUT_CAPACITY.get(program)
    if cap is None:
        raise ExecutorError(f"unknown program '{program}'")
    return cap


def _readback_length(
    program: str, params: bytes, mem: wasm.Memory, store: wasm.Store, slot_ptr: int
) -> int:
    """Final output length after a successful run."""
    if program == _FILTER:
        raw = bytes(mem.read(store, slot_ptr, slot_ptr + 4))
        return int.from_bytes(raw, "big")
    return _out_capacity(program, params, 0)


def _make_config() -> wasm.Config:
    """Deterministic, capability-free host configuration.

    Fuel metering on; no SIMD/relaxed-SIMD, no threads, no shared memory,
    no memory64, no GC/component model/exceptions/tail-call/stack switching,
    no wide arithmetic, no custom page sizes. WASI is never configured and no
    host functions are ever defined, so guests get no filesystem/clock/
    network/random — any module that imports something fails to instantiate.
    """
    cfg = wasm.Config()
    cfg.consume_fuel = True
    cfg.wasm_simd = False
    cfg.wasm_relaxed_simd = False
    cfg.wasm_threads = False
    cfg.shared_memory = False
    cfg.wasm_memory64 = False
    cfg.wasm_component_model = False
    cfg.wasm_gc = False
    cfg.wasm_exceptions = False
    cfg.wasm_tail_call = False
    cfg.wasm_stack_switching = False
    cfg.wasm_wide_arithmetic = False
    cfg.wasm_custom_page_sizes = False
    return cfg


def _check_sandbox(module: wasm.Module) -> None:
    """Fail closed: the guest must declare zero imports."""
    imports = module.imports
    if len(imports) != 0:
        names = sorted(f"{i.module}.{i.name}" for i in imports)
        raise ExecutorError(
            f"guest declares {len(imports)} import(s) — refusing to run a "
            f"guest with ambient capabilities: {names}"
        )


def execute(
    program: str,
    input_path: Path,
    params_path: Path,
    fuel: int = DEFAULT_FUEL,
    wasm_dir: Path | None = None,
    input_format: str = "pickle-bytes",
) -> RunResult:
    """Run one guest job and return its raw result (nothing is written).

    Raises GuestError(code) for guest error codes/traps, ExecutorError for
    host-side problems, FormatError for input decode failures.
    """
    if program not in PARAM_SIZES:
        raise ExecutorError(
            f"unknown program '{program}' (expected one of {sorted(PARAM_SIZES)})"
        )

    params = load_params_bytes(params_path)
    expected = PARAM_SIZES[program]
    if len(params) != expected:
        raise ExecutorError(
            f"{program}: params must be exactly {expected} bytes (Spec.md), "
            f"got {len(params)}"
        )

    # Decode the input file through the selected pluggable format.
    # (InputFormat.get returns the class; formats are stateless, so a fresh
    # instance per run is fine.)
    input_data = InputFormat.get(input_format)().load(
        input_path, program=program, params=params
    )

    wasm_path = find_wasm(program, wasm_dir)

    try:
        engine = wasm.Engine(_make_config())
        module = wasm.Module.from_file(engine, str(wasm_path))
    except wasm.WasmtimeError as exc:
        raise ExecutorError(f"cannot compile guest {wasm_path}: {exc}") from exc
    _check_sandbox(module)

    # One fresh store per run: deterministic fuel, no state carried between
    # jobs, guest memory (and thus the dlmalloc heap) starts clean.
    store = wasm.Store(engine)
    store.set_fuel(fuel)
    try:
        instance = wasm.Instance(store, module, [])
    except (wasm.WasmtimeError, wasm.Trap) as exc:
        raise ExecutorError(f"instantiation failed for {wasm_path}: {exc}") from exc
    exports = instance.exports(store)

    try:
        alloc_fn = exports["alloc"]
        run_fn = exports["run"]
        mem = exports["memory"]
    except KeyError as exc:
        raise ExecutorError(
            f"guest {wasm_path} missing required export {exc.args[0]!r}"
        ) from exc
    if not isinstance(alloc_fn, wasm.Func) or not isinstance(run_fn, wasm.Func):
        raise ExecutorError("guest exports 'alloc'/'run' are not functions")
    if not isinstance(mem, wasm.Memory):
        raise ExecutorError("guest does not export a linear 'memory'")

    # ---- Stage params, input, out region and the 4-byte length slot, then
    # run. Every wasm call below (alloc *and* run) consumes fuel and can
    # trap, so they share one trap-mapping block. ----
    out_cap = _out_capacity(program, params, len(input_data))
    try:
        params_ptr = alloc_fn(store, len(params))
        mem.write(store, params, params_ptr)
        input_ptr = alloc_fn(store, len(input_data))
        mem.write(store, input_data, input_ptr)
        out_ptr = alloc_fn(store, out_cap)
        slot_ptr = alloc_fn(store, 4)
        ret = run_fn(store, input_ptr, len(input_data), params_ptr, len(params), out_ptr, slot_ptr)
    except wasm.Trap as trap:
        code = trap.trap_code
        if code == wasm.TrapCode.OUT_OF_FUEL:
            raise GuestError(1, "E_FUEL_EXHAUSTED: guest ran out of fuel") from trap
        hint = ""
        if code == wasm.TrapCode.UNREACHABLE and out_cap > 0:
            hint = (
                " (a guest panic or a failed dlmalloc allocation traps as "
                "'unreachable' — check heap exhaustion / engine memory limits "
                "if the input is large)"
            )
        raise ExecutorError(
            f"guest trapped: {trap.message} (code={code}){hint}"
        ) from trap

    ret = int(ret)
    if ret != 0:
        name = ERROR_NAMES.get(ret, "E_UNKNOWN")
        raise GuestError(ret, f"guest returned {name} ({ret})")

    # ---- Read back the output ----
    out_len = _readback_length(program, params, mem, store, slot_ptr)
    if out_len > out_cap:
        raise ExecutorError(
            f"guest reported output length {out_len} beyond allocated "
            f"capacity {out_cap} — corrupted guest or ABI mismatch"
        )
    output = bytes(mem.read(store, out_ptr, out_ptr + out_len))

    return RunResult(program=program, params=params, output=output)

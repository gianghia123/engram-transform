"""Built-in raw-bytes formats: the simplest input and the default output."""

from __future__ import annotations

from pathlib import Path

from .base import FormatError, InputFormat, OutputFormat, RunResult


class RawBytesInput(InputFormat):
    """The input file IS the guest input blob, read verbatim (no pickle)."""

    name = "raw-bytes"

    def load(self, path: Path, *, program: str, params: bytes) -> bytes:
        try:
            return path.read_bytes()
        except OSError as exc:
            raise FormatError(f"cannot read input {path}: {exc}") from exc


class RawBytesOutput(OutputFormat):
    """Writes the raw guest output bytes to ``<out_dir>/output.bin``."""

    name = "raw"

    def write(self, result: RunResult, out_dir: Path) -> Path:
        out_dir.mkdir(parents=True, exist_ok=True)
        out_path = out_dir / "output.bin"
        try:
            out_path.write_bytes(result.output)
        except OSError as exc:
            raise FormatError(f"cannot write {out_path}: {exc}") from exc
        return out_path

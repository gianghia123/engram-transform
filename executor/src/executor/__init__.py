"""Engram-Transform executor: run a guest program in a sandboxed host.

Usage:
    executor <program> --input <file> --params <params.bin> \
             [--input-format NAME] [--output-format NAME] \
             [--out-dir DIR] [--fuel N] [--wasm-dir DIR]
    executor --list-formats

<program>         count | filter | histogram   (checksum: not implemented yet)
--input           input file in --input-format encoding
--params          raw spec param blob (version byte 0x01 first)
--input-format    decoder for --input (default: pickle-bytes); see --list-formats
--output-format   writer for the result (default: raw); see --list-formats
--out-dir         where the output format writes (default: current directory)
--fuel            deterministic fuel budget per run (default 1_000_000_000)
--wasm-dir        override guest wasm lookup (directory or .wasm file)

Exit codes: 0 = E_OK; 1..6 = Spec.md guest error code; 64 = host-side error;
2 = CLI usage error.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .engine import ExecutorError, GuestError, execute
from .formats import FormatError, InputFormat, OutputFormat

# Programs whose guest .wasm can actually be built/loaded right now.
KNOWN = ("count", "filter", "histogram", "checksum")


def _list_formats() -> None:
    print("input formats:  " + ", ".join(InputFormat.available()))
    print("output formats: " + ", ".join(OutputFormat.available()))


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        prog="executor",
        description="Run an Engram-Transform guest program in a sandboxed host.",
    )
    parser.add_argument("program", metavar="PROGRAM", nargs="?", help="one of: " + ", ".join(KNOWN))
    parser.add_argument("--input", type=Path, help="input file (encoded per --input-format)")
    parser.add_argument("--params", type=Path, help="raw spec params blob")
    parser.add_argument(
        "--input-format",
        default="pickle-bytes",
        choices=InputFormat.available(),
        help="input decoder (default: pickle-bytes)",
    )
    parser.add_argument(
        "--output-format",
        default="raw",
        choices=OutputFormat.available(),
        help="result writer (default: raw)",
    )
    parser.add_argument("--out-dir", type=Path, default=Path("."), help="output directory (default: .)")
    parser.add_argument("--fuel", type=int, default=1_000_000_000, help="deterministic fuel budget")
    parser.add_argument(
        "--wasm-dir",
        type=Path,
        default=None,
        help="guest wasm directory or file (default: auto-detect debug/release)",
    )
    parser.add_argument(
        "--list-formats",
        action="store_true",
        help="list available input/output formats and exit",
    )
    args = parser.parse_args(argv)

    if args.list_formats:
        _list_formats()
        sys.exit(0)
    if args.program is None:
        parser.error("PROGRAM is required (or use --list-formats)")
    if args.program not in KNOWN:
        parser.error(f"unknown program {args.program!r}; expected one of: {', '.join(KNOWN)}")
    if args.input is None or args.params is None:
        parser.error("--input and --params are required")
    if args.program == "checksum":
        print(
            "checksum: guest program not implemented yet (hash functions pending) — "
            "nothing to execute.",
            file=sys.stderr,
        )
        sys.exit(64)
    if args.fuel < 0:
        parser.error("--fuel must be >= 0")

    try:
        result = execute(
            program=args.program,
            input_path=args.input,
            params_path=args.params,
            fuel=args.fuel,
            wasm_dir=args.wasm_dir,
            input_format=args.input_format,
        )
        out_path = OutputFormat.get(args.output_format)().write(result, args.out_dir)
    except GuestError as exc:
        print(f"executor: {exc}", file=sys.stderr)
        sys.exit(exc.code)
    except (ExecutorError, FormatError) as exc:
        print(f"executor: {exc}", file=sys.stderr)
        sys.exit(64)

    # Success: E_OK. The result is on disk — stdout stays quiet so the
    # written file is the single source of truth for repeatability tests.
    print(f"E_OK: {out_path}")
    sys.exit(0)


if __name__ == "__main__":
    main()

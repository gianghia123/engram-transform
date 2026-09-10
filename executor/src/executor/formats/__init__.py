"""Pluggable input/output formats.

Importing this package loads every module under ``executor/formats/`` so that
each ``InputFormat`` / ``OutputFormat`` subclass registers itself. Drop a new
child class into any module here and it is available immediately.
"""

from __future__ import annotations

import importlib
import pkgutil

from . import base  # noqa: F401  (define registries before anything registers)
from .base import FormatError, InputFormat, OutputFormat, RunResult

__all__ = ["FormatError", "InputFormat", "OutputFormat", "RunResult"]


def _load_all() -> None:
    for module_info in pkgutil.iter_modules(__path__):
        if module_info.name == "base" or module_info.name.startswith("_"):
            continue
        importlib.import_module(f"{__name__}.{module_info.name}")


_load_all()

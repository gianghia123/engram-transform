"""Base classes and registry for pluggable input/output formats.

How to add a format: write a module anywhere under ``executor/formats/``
containing a subclass of ``InputFormat`` (or ``OutputFormat``) with a unique
non-empty ``name``. Subclasses register themselves automatically via
``__init_subclass__`` when the package loads; nothing else needs editing:

    # executor/formats/example.py
    from .base import InputFormat, FormatError

    class ExampleInput(InputFormat):
        name = "example"

        def load(self, path, *, program, params):
            ...  # return the raw bytes blob the guest should receive

Then:  executor count --input f.example --input-format example ...
"""

from __future__ import annotations

import abc
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar


class FormatError(Exception):
    """Raised when an input cannot be decoded or an output cannot be written."""


class _Registry:
    """Name -> class lookup shared by all formats of one kind."""

    def __init__(self) -> None:
        self._by_name: dict[str, type] = {}

    def add(self, fmt: type) -> None:
        name = fmt.name
        if not name:
            raise TypeError(
                f"{fmt.__module__}.{fmt.__qualname__} must define a unique non-empty `name`"
            )
        existing = self._by_name.get(name)
        if existing is not None:
            raise TypeError(
                f"duplicate format name {name!r}: already defined by "
                f"{existing.__module__}.{existing.__qualname__}"
            )
        self._by_name[name] = fmt

    def get(self, name: str) -> type:
        try:
            return self._by_name[name]
        except KeyError:
            available = ", ".join(sorted(self._by_name)) or "(none)"
            raise FormatError(f"unknown {self._kind} format {name!r}; available: {available}")

    def names(self) -> list[str]:
        return sorted(self._by_name)

    _kind: str = ""


class _InputRegistry(_Registry):
    _kind = "input"


class _OutputRegistry(_Registry):
    _kind = "output"


class InputFormat(abc.ABC):
    """Decodes one on-disk input file into the raw input blob for the guest.

    ``load`` receives the file path plus execution context so a format can
    encode records correctly (e.g. an int-list format needs params'
    field_width/signedness); formats that don't care ignore the extras.
    """

    name: ClassVar[str] = ""
    _registry: ClassVar[_Registry] = _InputRegistry()

    def __init_subclass__(cls, **kwargs: object) -> None:
        super().__init_subclass__(**kwargs)
        cls._registry.add(cls)

    @classmethod
    def get(cls, name: str) -> "type[InputFormat]":
        return cls._registry.get(name)

    @classmethod
    def available(cls) -> list[str]:
        return cls._registry.names()

    @abc.abstractmethod
    def load(self, path: Path, *, program: str, params: bytes) -> bytes:
        """Return the exact bytes that become the guest's input blob."""
        raise NotImplementedError


@dataclass(frozen=True)
class RunResult:
    """Everything an output format needs to materialize a run's result."""

    program: str
    params: bytes  # the raw spec param blob this run used
    output: bytes  # raw bytes the guest produced (E_OK only)


class OutputFormat(abc.ABC):
    """Materializes a successful run's result onto disk.

    ``write`` returns the path(s) it created. It is only called for E_OK
    runs; guest error codes are surfaced as process exit codes before any
    output format runs.
    """

    name: ClassVar[str] = ""
    _registry: ClassVar[_Registry] = _OutputRegistry()

    def __init_subclass__(cls, **kwargs: object) -> None:
        super().__init_subclass__(**kwargs)
        cls._registry.add(cls)

    @classmethod
    def get(cls, name: str) -> "type[OutputFormat]":
        return cls._registry.get(name)

    @classmethod
    def available(cls) -> list[str]:
        return cls._registry.names()

    @abc.abstractmethod
    def write(self, result: RunResult, out_dir: Path) -> Path:
        """Write the result under ``out_dir`` and return the primary path."""
        raise NotImplementedError

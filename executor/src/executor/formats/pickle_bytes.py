"""Default input format: a pickle file containing one raw ``bytes`` blob.

SECURITY NOTE: ``pickle.load`` executes arbitrary code during unpickling.
This is a local dev/testing tool — only ever load pickle files you created.
"""

from __future__ import annotations

import pickle
from pathlib import Path

from .base import FormatError, InputFormat


class PickleBytesInput(InputFormat):
    """Unpickles ``path``; the object must be one ``bytes`` (or ``bytearray``)
    blob, which becomes the guest's input verbatim."""

    name = "pickle-bytes"

    def load(self, path: Path, *, program: str, params: bytes) -> bytes:
        try:
            with open(path, "rb") as f:
                data = pickle.load(f)
        except Exception as exc:  # pickle protocol errors, EOF, truncation...
            raise FormatError(f"cannot unpickle input {path}: {exc}") from exc
        if isinstance(data, bytearray):
            return bytes(data)
        if not isinstance(data, bytes):
            raise FormatError(
                f"input {path} must pickle a single `bytes` blob, "
                f"got {type(data).__name__}"
            )
        return data

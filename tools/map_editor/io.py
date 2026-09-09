"""Map editor file IO."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path

from .formatting import format_map_file
from .normalization import normalize_map


def read_map(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    return normalize_map(data["map"])


def write_map(path: Path, map_data: dict) -> None:
    wrapper = {"map": normalize_map(map_data)}
    text = format_map_file(wrapper) + "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
        os.replace(tmp_name, path)
    except Exception:
        try:
            os.unlink(tmp_name)
        except FileNotFoundError:
            pass
        raise

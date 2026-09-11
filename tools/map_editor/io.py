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
    result = normalize_map(data["map"])
    if "editor_settings" in data:
        result["_settings"] = data["editor_settings"]
    return result


def write_map(path: Path, map_data: dict, *, recovery: bool = False) -> None:
    wrapper = {"map": normalize_map(map_data)}
    text = format_map_file(wrapper) + "\n"
    if recovery and "_settings" in map_data:
        text = text.rstrip()[:-1] + ',\n  "editor_settings": ' + json.dumps(map_data["_settings"], indent=2) + "\n}\n"
    write_text_atomic(path, text)


def write_text_atomic(path: Path, text: str) -> None:
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

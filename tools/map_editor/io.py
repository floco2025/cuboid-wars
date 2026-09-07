"""Map editor file IO and default-map construction."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path

from .constants import DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS
from .formatting import format_map_file
from .normalization import empty_level, normalize_map


def empty_map(grid_cols: int = DEFAULT_GRID_COLS, grid_rows: int = DEFAULT_GRID_ROWS) -> dict:
    # No seeded actor zone: there's no default kind to give it. Users paint
    # actor zones explicitly and pick a kind in the dialog.
    # The player-spawn-zone seed in the top-left guarantees the map is
    # save-valid out of the box (at least one player spawn zone is required).
    return {
        "grid_cols": grid_cols,
        "grid_rows": grid_rows,
        "actor_spawn_zones": [],
        "player_spawn_zones": [
            {"level": 0, "cols": [0, min(2, grid_cols)], "rows": [0, min(2, grid_rows)]},
        ],
        "items": [],
        "pressure_plates": [],
        "levels": [empty_level(0)],
        "ramps": [],
        "ladders": [],
        "nested_maps": [],
    }


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

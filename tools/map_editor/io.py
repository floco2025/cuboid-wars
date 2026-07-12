"""Map editor file IO and default-map construction."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path

from .constants import DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS, REPO_ROOT, SUPPORTED_VERSION
from .formatting import format_map_file
from .geometry import canonicalize_map, normalize_map


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
            {"level": 0, "cols": [0, 2], "rows": [0, 2]},
        ],
        "cookie_spawn_zones": [],
        "key_spawn_zones": [],
        "pressure_plates": [],
        "levels": [
            {
                "name": "Level 0",
                "floors": [],
                "inaccessible_floors": [],
                "grass": [],
                "walls": [],
                "barriers": [],
                "lights": [],
            }
        ],
        "ramps": [],
    }


def read_map(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if data.get("version") != SUPPORTED_VERSION:
        raise ValueError(f"unsupported map file version {data.get('version')!r}")
    return canonicalize_map(normalize_map(data["map"]))


def load_materials_catalog() -> list[str]:
    """Return the sorted list of material *role* names from `assets.json`'s
    `aliases` block. Roles are what map files reference and what the user
    picks in the editor; the underlying texture material IDs are an
    implementation detail of the renderer. Falls back to the raw `materials`
    keys if no aliases are defined. Returns an empty list if the file can't
    be located — callers handle that gracefully."""
    catalog_path = REPO_ROOT / "config" / "client" / "assets.json"
    if catalog_path.exists():
        try:
            with catalog_path.open("r", encoding="utf-8") as handle:
                assets = json.load(handle)
            aliases = assets.get("aliases") or {}
            if aliases:
                return sorted(aliases.keys())
            materials = assets.get("materials") or {}
            return sorted(materials.keys())
        except (OSError, json.JSONDecodeError):
            pass
    return []


def write_map(path: Path, map_data: dict) -> None:
    wrapper = {"version": SUPPORTED_VERSION, "map": canonicalize_map(map_data)}
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

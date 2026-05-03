#!/usr/bin/env python3
"""Native editor for Cuboid Wars map source files."""

from __future__ import annotations

import argparse
import copy
import json
import math
import os
import signal
import sys
import tempfile
from pathlib import Path

import hashlib

from PySide6.QtCore import QPoint, QPointF, QRectF, QSize, Qt, QTimer
from PySide6.QtGui import (
    QAction,
    QBrush,
    QColor,
    QKeySequence,
    QPainter,
    QPen,
    QShortcut,
    QUndoCommand,
    QUndoStack,
)
from PySide6.QtWidgets import (
    QApplication,
    QComboBox,
    QFileDialog,
    QInputDialog,
    QLabel,
    QMainWindow,
    QMenu,
    QMessageBox,
    QSizePolicy,
    QToolBar,
    QWidget,
)


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MAP = REPO_ROOT / "config" / "server" / "map.json"
SUPPORTED_VERSION = 1

MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Inaccessible Floor"
MODE_ACTOR_SPAWN_PAINT = "Actor Spawn Zone (Paint)"
MODE_PLAYER_SPAWN_PAINT = "Player Spawn Zone (Paint)"
MODE_SPAWN_ZONE_EDIT = "Spawn Zone (Edit)"
MODE_WALL = "Wall"
MODE_RAMP_UP = "Ramp (Up)"
MODE_RAMP_DOWN = "Ramp (Down)"
MODE_ERASE = "Erase"
MODE_ERASE_KEEP_FLOORS = "Erase (Keep Floors)"
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
ERASE_MODES = (MODE_ERASE, MODE_ERASE_KEEP_FLOORS)
SPAWN_PAINT_MODES = (MODE_ACTOR_SPAWN_PAINT, MODE_PLAYER_SPAWN_PAINT)
FLOOR_HIT_KINDS = ("Floor", "Inaccessible Floor")
MODES = [
    MODE_FLOOR,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ACTOR_SPAWN_PAINT,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_SPAWN_ZONE_EDIT,
    MODE_WALL,
    MODE_RAMP_UP,
    MODE_RAMP_DOWN,
    MODE_ERASE,
    MODE_ERASE_KEEP_FLOORS,
]

# Two named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
PLAYER_ZONE_LIST = "player_spawn_zones"
SPAWN_ZONE_LISTS = (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST)

DEFAULT_ACTOR_SPAWN_ENTRIES = [{"kind": "actor", "count": 1}]
SPAWN_ZONE_HANDLE_PIXELS = 8.0

MIN_CELL = 12.0
EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20


def empty_map() -> dict:
    return {
        "grid_cols": DEFAULT_GRID_COLS,
        "grid_rows": DEFAULT_GRID_ROWS,
        "actor_spawn_zones": [
            {
                "level": 0,
                "cols": [0, 2],
                "rows": [0, 2],
                "spawns": [dict(entry) for entry in DEFAULT_ACTOR_SPAWN_ENTRIES],
            },
        ],
        "player_spawn_zones": [
            {"level": 0, "cols": [0, 2], "rows": [0, 2]},
        ],
        "levels": [{"name": "Level 0", "floors": [], "inaccessible_floors": [], "walls": []}],
        "ramps": [],
    }


def read_map(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if data.get("version") != SUPPORTED_VERSION:
        raise ValueError(f"unsupported map file version {data.get('version')!r}")
    return canonicalize_map(normalize_map(data["map"]))


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


def json_scalar(value) -> str:
    return json.dumps(value, separators=(",", ": "))


def format_point(point: list[int]) -> str:
    return "[" + ", ".join(str(v) for v in point) + "]"


def format_point_array(name: str, points: list[list[int]], indent: int) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not points:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    for idx, point in enumerate(points):
        comma = "," if idx + 1 < len(points) else ""
        lines.append(f"{inner}{format_point(point)}{comma}")
    lines.append(f"{pad}]")
    return lines


def with_trailing_comma(lines: list[str]) -> list[str]:
    if lines:
        lines[-1] += ","
    return lines


def format_ramp(ramp: dict, indent: int) -> str:
    pad = " " * indent
    return (
        f'{pad}{{"lower_level": {ramp["lower_level"]}, '
        f'"low": {format_point(ramp["low"])}, '
        f'"high": {format_point(ramp["high"])}}}'
    )


def format_actor_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not zones:
        return [f'{pad}"actor_spawn_zones": []']
    lines = [f'{pad}"actor_spawn_zones": [']
    for idx, zone in enumerate(zones):
        comma = "," if idx + 1 < len(zones) else ""
        spawns = ", ".join(
            f'{{"kind": {json.dumps(entry["kind"])}, "count": {entry["count"]}}}'
            for entry in zone["spawns"]
        )
        lines.append(
            f'{inner}{{"level": {zone["level"]}, '
            f'"cols": [{zone["cols"][0]}, {zone["cols"][1]}], '
            f'"rows": [{zone["rows"][0]}, {zone["rows"][1]}], '
            f'"spawns": [{spawns}]}}{comma}'
        )
    lines.append(f"{pad}]")
    return lines


def format_player_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not zones:
        return [f'{pad}"player_spawn_zones": []']
    lines = [f'{pad}"player_spawn_zones": [']
    for idx, zone in enumerate(zones):
        comma = "," if idx + 1 < len(zones) else ""
        lines.append(
            f'{inner}{{"level": {zone["level"]}, '
            f'"cols": [{zone["cols"][0]}, {zone["cols"][1]}], '
            f'"rows": [{zone["rows"][0]}, {zone["rows"][1]}]}}{comma}'
        )
    lines.append(f"{pad}]")
    return lines


def format_map_file(wrapper: dict) -> str:
    map_data = wrapper["map"]
    lines = [
        "{",
        f'  "version": {wrapper["version"]},',
        '  "map": {',
        f'    "grid_cols": {map_data["grid_cols"]},',
        f'    "grid_rows": {map_data["grid_rows"]},',
        *with_trailing_comma(format_actor_spawn_zones(map_data["actor_spawn_zones"], 4)),
        *with_trailing_comma(format_player_spawn_zones(map_data["player_spawn_zones"], 4)),
        '    "levels": [',
    ]

    for level_idx, level in enumerate(map_data["levels"]):
        lines.extend(
            [
                "      {",
                f'        "name": {json_scalar(level["name"])},',
                *with_trailing_comma(format_point_array("floors", level["floors"], 8)),
                *with_trailing_comma(format_point_array("inaccessible_floors", level["inaccessible_floors"], 8)),
                *format_point_array("walls", level["walls"], 8),
                "      }" + ("," if level_idx + 1 < len(map_data["levels"]) else ""),
            ]
        )

    lines.append("    ],")
    if map_data["ramps"]:
        lines.append('    "ramps": [')
        for idx, ramp in enumerate(map_data["ramps"]):
            comma = "," if idx + 1 < len(map_data["ramps"]) else ""
            lines.append(format_ramp(ramp, 6) + comma)
        lines.append("    ]")
    else:
        lines.append('    "ramps": []')
    lines.append("  }")
    lines.append("}")
    return "\n".join(lines)


def normalize_map(map_data: dict) -> dict:
    cols = int(map_data.get("grid_cols", DEFAULT_GRID_COLS))
    rows = int(map_data.get("grid_rows", DEFAULT_GRID_ROWS))
    actor_spawn_zones = [normalize_actor_spawn_zone(z) for z in map_data.get("actor_spawn_zones", [])]
    player_spawn_zones = [normalize_player_spawn_zone(z) for z in map_data.get("player_spawn_zones", [])]
    levels = []
    for idx, level in enumerate(map_data.get("levels", [])):
        levels.append(
            {
                "name": str(level.get("name") or f"Level {idx}"),
                "floors": [[int(c), int(r)] for c, r in level.get("floors", [])],
                "inaccessible_floors": [
                    [int(c), int(r)] for c, r in level.get("inaccessible_floors", [])
                ],
                "walls": [[int(c0), int(r0), int(c1), int(r1)] for c0, r0, c1, r1 in level.get("walls", [])],
            }
        )
    if not levels:
        levels = [{"name": "Level 0", "floors": [], "inaccessible_floors": [], "walls": []}]

    ramps = []
    for ramp in map_data.get("ramps", []):
        low = ramp["low"]
        high = ramp["high"]
        ramps.append(
            {
                "low": [int(low[0]), int(low[1])],
                "high": [int(high[0]), int(high[1])],
                "lower_level": int(ramp["lower_level"]),
            }
        )
    return {
        "grid_cols": cols,
        "grid_rows": rows,
        "actor_spawn_zones": actor_spawn_zones,
        "player_spawn_zones": player_spawn_zones,
        "levels": levels,
        "ramps": ramps,
    }


def normalize_actor_spawn_zone(zone: dict) -> dict:
    cols = zone.get("cols") or [0, 0]
    rows = zone.get("rows") or [0, 0]
    raw_spawns = zone.get("spawns") or []
    spawns = []
    for entry in raw_spawns:
        if not isinstance(entry, dict):
            continue
        kind = str(entry.get("kind", ""))
        try:
            count = int(entry.get("count", 0))
        except (TypeError, ValueError):
            count = 0
        if not kind:
            continue
        spawns.append({"kind": kind, "count": max(0, count)})
    return {
        "level": int(zone.get("level", 0)),
        "cols": [int(cols[0]), int(cols[1])],
        "rows": [int(rows[0]), int(rows[1])],
        "spawns": spawns,
    }


def normalize_player_spawn_zone(zone: dict) -> dict:
    cols = zone.get("cols") or [0, 0]
    rows = zone.get("rows") or [0, 0]
    return {
        "level": int(zone.get("level", 0)),
        "cols": [int(cols[0]), int(cols[1])],
        "rows": [int(rows[0]), int(rows[1])],
    }


def actor_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
        tuple((entry["kind"], entry["count"]) for entry in zone["spawns"]),
    )


def player_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
    )


def zone_key(list_name: str, zone: dict) -> tuple:
    if list_name == ACTOR_ZONE_LIST:
        return actor_zone_key(zone)
    return player_zone_key(zone)


def _dedupe_sorted(zones: list[dict], key_fn) -> list[dict]:
    seen = set()
    out = []
    for zone in sorted(zones, key=key_fn):
        k = key_fn(zone)
        if k in seen:
            continue
        seen.add(k)
        out.append(zone)
    return out


def canonicalize_map(map_data: dict) -> dict:
    b = normalize_map(copy.deepcopy(map_data))
    enforce_ramp_floor_rules(b)
    for zone in b["actor_spawn_zones"]:
        # dedupe entries by kind, keeping the last value
        by_kind: dict[str, int] = {}
        for entry in zone["spawns"]:
            by_kind[entry["kind"]] = entry["count"]
        zone["spawns"] = [
            {"kind": kind, "count": count}
            for kind, count in sorted(by_kind.items())
        ]
    b["actor_spawn_zones"] = _dedupe_sorted(b["actor_spawn_zones"], actor_zone_key)
    b["player_spawn_zones"] = _dedupe_sorted(b["player_spawn_zones"], player_zone_key)
    for level in b["levels"]:
        floors = {(c, r) for c, r in level["floors"]}
        inaccessible_floors = {(c, r) for c, r in level["inaccessible_floors"]} - floors
        level["floors"] = [[c, r] for c, r in sorted(floors, key=lambda p: (p[1], p[0]))]
        level["inaccessible_floors"] = [
            [c, r] for c, r in sorted(inaccessible_floors, key=lambda p: (p[1], p[0]))
        ]

        walls = {tuple(normalized_wall(wall)) for wall in level["walls"]}
        level["walls"] = [list(wall) for wall in sorted(walls)]

    ramp_keys = {
        (ramp["lower_level"], tuple(ramp["low"]), tuple(ramp["high"]))
        for ramp in b["ramps"]
    }
    b["ramps"] = [
        {"lower_level": lower, "low": list(low), "high": list(high)}
        for lower, low, high in sorted(ramp_keys)
    ]
    return b


def enforce_ramp_floor_rules(map_data: dict) -> None:
    for ramp in map_data["ramps"]:
        lower = ramp["lower_level"]
        upper = lower + 1
        if lower < 0 or upper >= len(map_data["levels"]):
            continue
        cells = ramp_cells(ramp)
        if not cells:
            continue

        lower_floors = {tuple(floor) for floor in map_data["levels"][lower]["floors"]}
        upper_floors = {tuple(floor) for floor in map_data["levels"][upper]["floors"]}
        lower_inaccessible_floors = {tuple(floor) for floor in map_data["levels"][lower]["inaccessible_floors"]}
        upper_inaccessible_floors = {tuple(floor) for floor in map_data["levels"][upper]["inaccessible_floors"]}
        lower_floors.update(cells)
        upper_floors.difference_update(cells)
        lower_inaccessible_floors.difference_update(cells)
        upper_inaccessible_floors.difference_update(cells)
        map_data["levels"][lower]["floors"] = [[c, r] for c, r in lower_floors]
        map_data["levels"][upper]["floors"] = [[c, r] for c, r in upper_floors]
        map_data["levels"][lower]["inaccessible_floors"] = [[c, r] for c, r in lower_inaccessible_floors]
        map_data["levels"][upper]["inaccessible_floors"] = [[c, r] for c, r in upper_inaccessible_floors]


def normalized_wall(wall: list[int]) -> list[int]:
    c0, r0, c1, r1 = wall
    if (c1, r1) < (c0, r0):
        return [c1, r1, c0, r0]
    return [c0, r0, c1, r1]


def validate_map(map_data: dict) -> list[str]:
    errors: list[str] = []
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    if cols <= 0 or rows <= 0:
        errors.append("grid_cols and grid_rows must be positive")
    if not map_data["levels"]:
        errors.append("at least one level is required")
    if not map_data["player_spawn_zones"]:
        errors.append("at least one player_spawn_zones entry is required by the Rust loader")

    for idx, zone in enumerate(map_data["actor_spawn_zones"]):
        _validate_zone_rect(zone, f"actor_spawn_zones[{idx}]", map_data, errors)
        if not zone["spawns"]:
            errors.append(f"actor_spawn_zones[{idx}] has empty `spawns` list")
        kinds_seen = set()
        for entry in zone["spawns"]:
            if not entry["kind"]:
                errors.append(f"actor_spawn_zones[{idx}] has an entry with empty kind")
            if entry["kind"] in kinds_seen:
                errors.append(f"actor_spawn_zones[{idx}] has duplicate kind {entry['kind']!r}")
            kinds_seen.add(entry["kind"])
            if entry["count"] < 0:
                errors.append(f"actor_spawn_zones[{idx}] kind {entry['kind']!r} has negative count")

    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _validate_zone_rect(zone, f"player_spawn_zones[{idx}]", map_data, errors)

    for level_idx, level in enumerate(map_data["levels"]):
        prefix = level_label(level, level_idx)
        if not level["floors"]:
            errors.append(f"{prefix}: at least one floor is required by the Rust loader")
        floor_set = {tuple(floor) for floor in level["floors"]}
        for floor in level["floors"]:
            c, r = floor
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: floor {floor} is outside the grid")
        for floor in level["inaccessible_floors"]:
            c, r = floor
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: inaccessible floor {floor} is outside the grid")
            if tuple(floor) in floor_set:
                errors.append(f"{prefix}: inaccessible floor {floor} overlaps a floor")
        for wall in level["walls"]:
            c0, r0, c1, r1 = wall
            if not (grid_point_in_bounds(c0, r0, cols, rows) and grid_point_in_bounds(c1, r1, cols, rows)):
                errors.append(f"{prefix}: wall {wall} is outside the grid-line bounds")
            if abs(c1 - c0) + abs(r1 - r0) != 1:
                errors.append(f"{prefix}: wall {wall} is not one grid edge")

    floors_by_level = [
        {tuple(floor) for floor in level["floors"]} for level in map_data["levels"]
    ]
    ramp_cells_by_level: list[set[tuple[int, int]]] = [set() for _ in map_data["levels"]]
    for ramp in map_data["ramps"]:
        lower = ramp["lower_level"]
        for cell in ramp_cells(ramp):
            for level in (lower, lower + 1):
                if 0 <= level < len(ramp_cells_by_level):
                    ramp_cells_by_level[level].add(cell)
    inaccessible_by_level = [
        {tuple(floor) for floor in level["inaccessible_floors"]} for level in map_data["levels"]
    ]
    for idx, zone in enumerate(map_data["actor_spawn_zones"]):
        _check_zone_clear_of_obstructions(
            zone, f"actor_spawn_zones[{idx}]", ramp_cells_by_level, inaccessible_by_level, errors
        )
    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _check_zone_clear_of_obstructions(
            zone, f"player_spawn_zones[{idx}]", ramp_cells_by_level, inaccessible_by_level, errors
        )
    # `floors_by_level` is no longer used but kept above for callers expecting it.
    _ = floors_by_level

    for ramp in map_data["ramps"]:
        msg = ramp_error(ramp["low"], ramp["high"], ramp["lower_level"], cols, rows, len(map_data["levels"]))
        if msg:
            errors.append(f"ramp {ramp}: {msg}")
    return errors


def _validate_zone_rect(zone: dict, label: str, map_data: dict, errors: list[str]) -> None:
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    if not (0 <= zone["level"] < len(map_data["levels"])):
        errors.append(f"{label} has an invalid level {zone['level']}")
    c0, c1 = zone["cols"]
    r0, r1 = zone["rows"]
    if c1 <= c0 or r1 <= r0:
        errors.append(f"{label} has an empty range cols={zone['cols']} rows={zone['rows']}")
    if not (0 <= c0 and c1 <= cols and 0 <= r0 and r1 <= rows):
        errors.append(f"{label} is outside the grid: cols={zone['cols']} rows={zone['rows']}")


def _check_zone_clear_of_obstructions(
    zone: dict,
    label: str,
    ramp_cells_by_level: list[set[tuple[int, int]]],
    inaccessible_by_level: list[set[tuple[int, int]]],
    errors: list[str],
) -> None:
    level = zone["level"]
    if not (0 <= level < len(ramp_cells_by_level)):
        return
    ramps_set = ramp_cells_by_level[level]
    inaccessible_set = inaccessible_by_level[level]
    for col, row in zone_cells(zone):
        if (col, row) in ramps_set:
            errors.append(f"{label} cell [{col}, {row}] overlaps a ramp on level {level}")
        if (col, row) in inaccessible_set:
            errors.append(f"{label} cell [{col}, {row}] overlaps an inaccessible floor on level {level}")


def zone_cells(zone: dict) -> list[tuple[int, int]]:
    c0, c1 = zone["cols"]
    r0, r1 = zone["rows"]
    return [(c, r) for r in range(r0, r1) for c in range(c0, c1)]


def zone_rect(zone: dict) -> tuple[int, int, int, int]:
    return zone["cols"][0], zone["rows"][0], zone["cols"][1], zone["rows"][1]


def zone_intersects_rect(zone: dict, rect: tuple[int, int, int, int]) -> bool:
    return rects_overlap(zone_rect(zone), rect)


def zone_contains_cell(zone: dict, col: int, row: int) -> bool:
    c0, r0, c1, r1 = zone_rect(zone)
    return c0 <= col < c1 and r0 <= row < r1


def zone_color(spawns: list[dict]) -> QColor:
    if not spawns:
        return QColor(34, 197, 94)
    rgb = [0.0, 0.0, 0.0]
    for entry in spawns:
        c = tag_color(entry["kind"])
        rgb[0] += c.redF()
        rgb[1] += c.greenF()
        rgb[2] += c.blueF()
    n = len(spawns)
    return QColor.fromRgbF(rgb[0] / n, rgb[1] / n, rgb[2] / n)


def tag_color(tag: str) -> QColor:
    digest = hashlib.md5(tag.encode("utf-8")).digest()
    hue = (digest[0] | (digest[1] << 8)) % 360
    color = QColor()
    color.setHsv(hue, 165, 220)
    return color


# Parse free-form text like "actor:3, player:16" or "actor 3; player 16"
# into a list of {"kind", "count"} entries. The default count is 1 if the
# user only types the kind name. Used by the paint and edit prompts.
def parse_spawn_entries_input(text: str) -> list[dict]:
    seen = set()
    entries = []
    for raw in text.replace(";", ",").replace("\n", ",").split(","):
        token = raw.strip()
        if not token:
            continue
        # Allow either "kind:count" or "kind count" or just "kind".
        if ":" in token:
            kind, _, count_str = token.partition(":")
        elif " " in token:
            kind, _, count_str = token.partition(" ")
        else:
            kind, count_str = token, "1"
        kind = kind.strip()
        count_str = count_str.strip() or "1"
        if not kind or kind in seen:
            continue
        try:
            count = max(0, int(count_str))
        except ValueError:
            continue
        seen.add(kind)
        entries.append({"kind": kind, "count": count})
    return entries


def format_spawn_entries(spawns: list[dict]) -> str:
    return ", ".join(f"{entry['kind']}:{entry['count']}" for entry in spawns)


def level_label(level: dict, index: int) -> str:
    name = level.get("name")
    return f"Level {index}" if not name else f"Level {index} ({name})"


def grid_point_in_bounds(col: int, row: int, cols: int, rows: int) -> bool:
    return 0 <= col <= cols and 0 <= row <= rows


def ramp_error(low: list[int], high: list[int], lower_level: int, cols: int, rows: int, level_count: int) -> str | None:
    if lower_level < 0 or lower_level + 1 >= level_count:
        return "lower_level must have an upper level"
    if not grid_point_in_bounds(low[0], low[1], cols, rows):
        return "low point is outside the grid-line bounds"
    if not grid_point_in_bounds(high[0], high[1], cols, rows):
        return "high point is outside the grid-line bounds"
    width = abs(high[0] - low[0])
    height = abs(high[1] - low[1])
    if width == 0 or height == 0:
        return "ramp must span a non-empty rectangular footprint"
    if width == height:
        return "ramp needs one clear longer axis"
    return None


def ramp_rect(ramp: dict) -> tuple[int, int, int, int]:
    low = ramp["low"]
    high = ramp["high"]
    return min(low[0], high[0]), min(low[1], high[1]), max(low[0], high[0]), max(low[1], high[1])


def ramp_cells(ramp: dict) -> set[tuple[int, int]]:
    c0, r0, c1, r1 = ramp_rect(ramp)
    return {(col, row) for row in range(r0, r1) for col in range(c0, c1)}


def ramp_axis(ramp: dict) -> str:
    low = ramp["low"]
    high = ramp["high"]
    dx = high[0] - low[0]
    dy = high[1] - low[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def opposite_direction(direction: str) -> str:
    return {
        "north": "south",
        "south": "north",
        "east": "west",
        "west": "east",
    }[direction]


class SetMapCommand(QUndoCommand):
    def __init__(self, window: "EditorWindow", text: str, before: dict, after: dict):
        super().__init__(text)
        self.window = window
        self.before = canonicalize_map(before)
        self.after = canonicalize_map(after)

    def undo(self) -> None:
        self.window.set_map(self.before, mark_dirty=True)

    def redo(self) -> None:
        self.window.set_map(self.after, mark_dirty=True)


class Canvas(QWidget):
    def __init__(self, window: "EditorWindow"):
        super().__init__()
        self.window = window
        self.drag_start_cell: tuple[int, int] | None = None
        self.drag_start_point: tuple[int, int] | None = None
        self.drag_current_cell: tuple[int, int] | None = None
        self.drag_current_point: tuple[int, int] | None = None
        self.setMouseTracking(True)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.DefaultContextMenu)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def minimumSizeHint(self):
        return super().minimumSizeHint().expandedTo(QSize(360, 360))

    def sizeHint(self):
        cols = max(1, self.window.map_data["grid_cols"])
        rows = max(1, self.window.map_data["grid_rows"])
        return QSize(cols * EDITOR_CELL, rows * EDITOR_CELL).expandedTo(self.minimumSizeHint())

    def cell_size(self) -> float:
        cols = max(1, self.window.map_data["grid_cols"])
        rows = max(1, self.window.map_data["grid_rows"])
        return max(MIN_CELL, min(self.width() / cols, self.height() / rows))

    def grid_bounds(self) -> tuple[float, float]:
        cell = self.cell_size()
        return self.window.map_data["grid_cols"] * cell, self.window.map_data["grid_rows"] * cell

    def point_to_cell(self, pos) -> tuple[int, int] | None:
        cell = self.cell_size()
        col = int(pos.x() // cell)
        row = int(pos.y() // cell)
        if 0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]:
            return col, row
        return None

    def point_to_grid_point(self, pos) -> tuple[int, int]:
        cell = self.cell_size()
        col = round(pos.x() / cell)
        row = round(pos.y() / cell)
        return (
            max(0, min(self.window.map_data["grid_cols"], col)),
            max(0, min(self.window.map_data["grid_rows"], row)),
        )

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor("#1f2328"))
        cell = self.cell_size()
        cols = self.window.map_data["grid_cols"]
        rows = self.window.map_data["grid_rows"]
        level_idx = self.window.current_level
        level = self.window.map_data["levels"][level_idx]

        painter.fillRect(QRectF(0, 0, cols * cell, rows * cell), QColor("#111418"))

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor("#454f5b"))
        for col, row in level["floors"]:
            painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))
        painter.setBrush(QColor("#454f5b"))
        for col, row in level["inaccessible_floors"]:
            rect = QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2)
            painter.drawRect(rect)
            painter.setPen(QPen(QColor("#94a3b8"), 1))
            painter.drawLine(rect.topLeft(), rect.bottomRight())
            painter.drawLine(rect.bottomLeft(), rect.topRight())
            painter.setPen(Qt.PenStyle.NoPen)

        for ramp in self.window.map_data["ramps"]:
            lower = ramp["lower_level"]
            if level_idx not in (lower, lower + 1):
                continue
            self.paint_ramp(painter, ramp, cell, lower == level_idx)

        self.paint_spawn_zones(painter, cell, level_idx)

        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.paint_spawn_zone_selection(painter, cell, level_idx)

        if (
            self.drag_start_cell
            and self.drag_current_cell
            and (
                self.window.mode in (MODE_FLOOR, *ERASE_MODES)
                or self.window.mode == MODE_INACCESSIBLE_FLOOR
                or self.window.mode in SPAWN_PAINT_MODES
            )
        ):
            c0, r0, c1, r1 = rect_from_cells(self.drag_start_cell, self.drag_current_cell)
            if self.window.mode == MODE_ERASE:
                color = QColor(248, 113, 113, 120)
            elif self.window.mode == MODE_ERASE_KEEP_FLOORS:
                color = QColor(251, 146, 60, 120)
            elif self.window.mode == MODE_FLOOR:
                color = QColor(111, 180, 255, 120)
            elif self.window.mode == MODE_INACCESSIBLE_FLOOR:
                color = QColor(148, 163, 184, 120)
            elif self.window.mode == MODE_PLAYER_SPAWN_PAINT:
                color = QColor(99, 102, 241, 120)
            else:
                color = QColor(34, 197, 94, 120)
            painter.setBrush(color)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))

        if (
            self.window.mode == MODE_SPAWN_ZONE_EDIT
            and self.window.spawn_zone_drag is not None
        ):
            self.paint_spawn_zone_drag_preview(painter, cell)

        if self.drag_start_point and self.drag_current_point and self.window.mode == MODE_WALL:
            end = snapped_wall_end(self.drag_start_point, self.drag_current_point)
            self.paint_wall_preview(painter, self.drag_start_point, end, cell)
        elif self.drag_start_cell and self.drag_current_cell and self.window.mode in RAMP_MODES:
            self.paint_ramp_preview(painter, self.drag_start_cell, self.drag_current_cell, cell)

        painter.setPen(QPen(QColor("#2e343b"), 1))
        for col in range(cols + 1):
            x = col * cell
            painter.drawLine(x, 0, x, rows * cell)
        for row in range(rows + 1):
            y = row * cell
            painter.drawLine(0, y, cols * cell, y)

        painter.setPen(QPen(QColor("#f1f5f9"), 4, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        for c0, r0, c1, r1 in level["walls"]:
            painter.drawLine(c0 * cell, r0 * cell, c1 * cell, r1 * cell)

    def paint_spawn_zones(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Player zones first so actor zones (which carry kind labels) sit on top.
        for zone in self.window.map_data["player_spawn_zones"]:
            if zone["level"] == level_idx:
                self.paint_player_spawn_zone(painter, zone, cell)
        for zone in self.window.map_data["actor_spawn_zones"]:
            if zone["level"] == level_idx:
                self.paint_actor_spawn_zone(painter, zone, cell)

    def paint_actor_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        rect = QRectF(c0 * cell + 2, r0 * cell + 2, (c1 - c0) * cell - 4, (r1 - r0) * cell - 4)
        outline_color = zone_color(zone["spawns"])
        fill_color = QColor(outline_color)
        fill_color.setAlpha(70)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        label = format_spawn_entries(zone["spawns"]) if zone["spawns"] else "(empty)"
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def paint_player_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        rect = QRectF(c0 * cell + 2, r0 * cell + 2, (c1 - c0) * cell - 4, (r1 - r0) * cell - 4)
        outline_color = tag_color("player")
        fill_color = QColor(outline_color)
        fill_color.setAlpha(70)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2, Qt.PenStyle.DashLine))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, "player")

    def paint_spawn_zone_selection(self, painter: QPainter, cell: float, level_idx: int) -> None:
        zone = self.window.selected_spawn_zone()
        if zone is None or zone["level"] != level_idx:
            return
        c0, r0, c1, r1 = zone_rect(zone)
        rect = QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QPen(QColor("#f1f5f9"), 2, Qt.PenStyle.SolidLine))
        painter.drawRect(rect.adjusted(1, 1, -1, -1))

        handle = SPAWN_ZONE_HANDLE_PIXELS
        painter.setBrush(QColor("#f1f5f9"))
        painter.setPen(QPen(QColor("#0f172a"), 1))
        for cx, cy in self.spawn_zone_handle_centers(zone, cell):
            painter.drawRect(QRectF(cx - handle / 2, cy - handle / 2, handle, handle))

    def paint_spawn_zone_drag_preview(self, painter: QPainter, cell: float) -> None:
        drag = self.window.spawn_zone_drag
        if drag is None:
            return
        candidate = self.window.spawn_zone_candidate_rect()
        if candidate is None:
            return
        c0, r0, c1, r1 = candidate
        rect = QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell)
        painter.setBrush(QColor(248, 250, 252, 70))
        painter.setPen(QPen(QColor("#f8fafc"), 2, Qt.PenStyle.DashLine))
        painter.drawRect(rect.adjusted(1, 1, -1, -1))

    def spawn_zone_handle_centers(self, zone: dict, cell: float) -> list[tuple[float, float]]:
        c0, r0, c1, r1 = zone_rect(zone)
        x0, y0 = c0 * cell, r0 * cell
        x1, y1 = c1 * cell, r1 * cell
        mx, my = (x0 + x1) / 2, (y0 + y1) / 2
        return [
            (x0, y0),
            (mx, y0),
            (x1, y0),
            (x1, my),
            (x1, y1),
            (mx, y1),
            (x0, y1),
            (x0, my),
        ]

    def paint_ramp(self, painter: QPainter, ramp: dict, cell: float, is_lower_level: bool) -> None:
        c0, r0, c1, r1 = ramp_rect(ramp)
        painter.setPen(QPen(QColor("#111827"), 1))
        painter.setBrush(QColor("#d97706") if is_lower_level else QColor("#8b5cf6"))
        painter.drawRect(QRectF(c0 * cell + 3, r0 * cell + 3, (c1 - c0) * cell - 6, (r1 - r0) * cell - 6))

        if is_lower_level:
            direction = ramp_axis(ramp)
            label = "UP"
        else:
            direction = opposite_direction(ramp_axis(ramp))
            label = "DOWN"
        start, end = orthogonal_arrow_points(c0, r0, c1, r1, direction, cell)
        self.draw_arrow(painter, start, end, QColor("#ffffff"))

        painter.setPen(QColor("#ffffff"))
        painter.drawText(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell), Qt.AlignmentFlag.AlignCenter, label)

    def paint_ramp_preview(self, painter: QPainter, start_cell: tuple[int, int], end_cell: tuple[int, int], cell: float) -> None:
        start_point, end_point = ramp_points_from_cells(start_cell, end_cell)
        c0, r0 = min(start_point[0], end_point[0]), min(start_point[1], end_point[1])
        c1, r1 = max(start_point[0], end_point[0]), max(start_point[1], end_point[1])
        painter.setPen(QPen(QColor("#fbbf24"), 2, Qt.PenStyle.DashLine))
        painter.setBrush(QColor(217, 119, 6, 90))
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))
        direction = draw_direction(start_cell, end_cell)
        start, end = orthogonal_arrow_points(c0, r0, c1, r1, direction, cell)
        self.draw_arrow(painter, start, end, QColor("#fbbf24"))

    def paint_wall_preview(self, painter: QPainter, start: tuple[int, int], end: tuple[int, int], cell: float) -> None:
        painter.setPen(QPen(QColor("#38bdf8"), 3, Qt.PenStyle.DashLine, Qt.PenCapStyle.RoundCap))
        painter.drawLine(start[0] * cell, start[1] * cell, end[0] * cell, end[1] * cell)

    def draw_arrow(self, painter: QPainter, start: tuple[float, float], end: tuple[float, float], color: QColor) -> None:
        painter.setPen(QPen(color, 3, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        painter.drawLine(start[0], start[1], end[0], end[1])
        dx = end[0] - start[0]
        dy = end[1] - start[1]
        length = math.hypot(dx, dy)
        if length < 1:
            return
        ux, uy = dx / length, dy / length
        px, py = -uy, ux
        size = min(18.0, max(8.0, length * 0.18))
        p1 = QPoint(round(end[0]), round(end[1]))
        p2 = QPoint(round(end[0] - ux * size + px * size * 0.45), round(end[1] - uy * size + py * size * 0.45))
        p3 = QPoint(round(end[0] - ux * size - px * size * 0.45), round(end[1] - uy * size - py * size * 0.45))
        painter.setBrush(color)
        painter.drawPolygon([p1, p2, p3])

    def mousePressEvent(self, event) -> None:
        if event.button() == Qt.MouseButton.RightButton:
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.window.begin_spawn_zone_edit_press(event.position(), self.cell_size())
            self.update()
            return
        self.drag_start_cell = self.point_to_cell(event.position())
        self.drag_current_cell = self.drag_start_cell
        self.drag_start_point = self.point_to_grid_point(event.position())
        self.drag_current_point = self.drag_start_point
        self.update()

    def mouseMoveEvent(self, event) -> None:
        if not (event.buttons() & Qt.MouseButton.LeftButton):
            return
        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.window.update_spawn_zone_edit_drag(event.position(), self.cell_size())
            self.update()
            return
        self.drag_current_cell = self.point_to_cell(event.position()) or self.drag_current_cell
        self.drag_current_point = self.point_to_grid_point(event.position())
        self.update()

    def mouseReleaseEvent(self, event) -> None:
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if self.window.mode == MODE_FLOOR and self.drag_start_cell and self.drag_current_cell:
            self.window.add_floor_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_INACCESSIBLE_FLOOR and self.drag_start_cell and self.drag_current_cell:
            self.window.add_inaccessible_floor_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_ACTOR_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.add_actor_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_PLAYER_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.add_player_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.window.commit_spawn_zone_edit_drag()
        elif self.window.mode == MODE_WALL and self.drag_start_point and self.drag_current_point:
            self.window.add_wall_line(self.drag_start_point, snapped_wall_end(self.drag_start_point, self.drag_current_point))
        elif self.window.mode in RAMP_MODES and self.drag_start_cell and self.drag_current_cell:
            self.window.add_ramp(self.drag_start_cell, self.drag_current_cell, self.window.mode)
        elif self.window.mode in ERASE_MODES:
            preserve_floors = self.window.mode == MODE_ERASE_KEEP_FLOORS
            if self.drag_start_cell and self.drag_current_cell and self.drag_start_cell != self.drag_current_cell:
                self.window.erase_cell_rect(self.drag_start_cell, self.drag_current_cell, preserve_floors)
            else:
                self.window.erase_at(event.position(), self.cell_size(), preserve_floors)
        self.clear_drag()
        self.update()

    def contextMenuEvent(self, event) -> None:
        menu = QMenu(self)
        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            picked = self.window.spawn_zone_at(event.pos(), self.cell_size())
            if picked is None:
                disabled = menu.addAction("No spawn zone here")
                disabled.setEnabled(False)
            else:
                self.window.set_selected_spawn_zone(picked)
                self.update()
                list_name, _ = picked
                if list_name == ACTOR_ZONE_LIST:
                    menu.addAction("Edit Spawns...", lambda: self.window.edit_selected_spawn_zone_spawns())
                menu.addAction("Delete Spawn Zone", lambda: self.window.delete_selected_spawn_zone())
            menu.exec(event.globalPos())
            return
        hit = self.window.hit_at(event.pos(), self.cell_size())
        preserve_floors = self.window.mode == MODE_ERASE_KEEP_FLOORS
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            menu.addAction(f"Erase {hit[0]}", lambda: self.window.erase_hit(hit, preserve_floors))
        else:
            disabled = menu.addAction("Nothing to erase")
            disabled.setEnabled(False)
        menu.exec(event.globalPos())

    def clear_drag(self) -> None:
        self.drag_start_cell = None
        self.drag_current_cell = None
        self.drag_start_point = None
        self.drag_current_point = None


class EditorWindow(QMainWindow):
    def __init__(self, path: Path):
        super().__init__()
        self.path: Path | None = path
        self.map_data = read_map(path) if path.exists() else empty_map()
        self.current_level = 0
        self.mode = MODE_FLOOR
        self.dirty = False
        self.undo_stack = QUndoStack(self)
        self.shortcuts = []
        self.recent_actor_spawn_entries: list[dict] = [dict(entry) for entry in DEFAULT_ACTOR_SPAWN_ENTRIES]
        # Selected zone is identified by (list_name, index). None = nothing selected.
        self.selected_spawn_zone_ref: tuple[str, int] | None = None
        # Drag state: (list_name, index, mode, origin_xy_in_cells, original_zone)
        self.spawn_zone_drag: tuple[str, int, str, tuple[float, float], dict] | None = None

        self.canvas = Canvas(self)
        self.setCentralWidget(self.canvas)
        self.setWindowTitle("Cuboid Wars Editor")

        self.level_combo = QComboBox()
        self.level_combo.currentIndexChanged.connect(self.select_level)
        self.mode_combo = QComboBox()
        self.mode_combo.addItems(MODES)
        self.mode_combo.currentTextChanged.connect(self.set_mode)
        self.status_label = QLabel()

        self.build_menus()
        self.build_toolbar()
        self.statusBar().addPermanentWidget(self.status_label)
        self.refresh_ui()
        self.resize_to_map()

    def build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&File")
        self.add_menu_action(file_menu, "&Open...", QKeySequence.StandardKey.Open, self.open_file)
        self.add_menu_action(file_menu, "&Save", QKeySequence.StandardKey.Save, self.save)
        self.add_menu_action(file_menu, "Save &As...", QKeySequence.StandardKey.SaveAs, self.save_as)
        file_menu.addSeparator()
        self.add_menu_action(file_menu, "&Quit", QKeySequence.StandardKey.Quit, self.close)

        edit_menu = self.menuBar().addMenu("&Edit")
        undo_action = self.undo_stack.createUndoAction(self, "&Undo")
        undo_action.setShortcuts(QKeySequence.StandardKey.Undo)
        edit_menu.addAction(undo_action)
        redo_action = self.undo_stack.createRedoAction(self, "&Redo")
        redo_action.setShortcuts(QKeySequence.StandardKey.Redo)
        edit_menu.addAction(redo_action)

        level_menu = self.menuBar().addMenu("&Level")
        self.add_menu_action(level_menu, "&Add Level", None, self.add_level)
        self.add_menu_action(level_menu, "&Rename Level...", None, self.rename_level)
        self.add_menu_action(level_menu, "&Remove Level", None, self.remove_level)

        help_menu = self.menuBar().addMenu("&Help")
        self.add_menu_action(help_menu, "Tool &Reference", None, self.show_tool_reference)

        self.add_shortcut(Qt.Key.Key_Up, self.next_level)
        self.add_shortcut(Qt.Key.Key_Down, self.previous_level)
        self.add_shortcut(Qt.Key.Key_Left, self.previous_tool)
        self.add_shortcut(Qt.Key.Key_Right, self.next_tool)

    def add_shortcut(self, key, callback) -> None:
        shortcut = QShortcut(QKeySequence(key), self)
        shortcut.setContext(Qt.ShortcutContext.WindowShortcut)
        shortcut.activated.connect(callback)
        self.shortcuts.append(shortcut)

    def add_menu_action(self, menu: QMenu, text: str, shortcut, callback) -> QAction:
        action = QAction(text, self)
        if shortcut is not None:
            action.setShortcut(shortcut)
        action.triggered.connect(callback)
        menu.addAction(action)
        return action

    def build_toolbar(self) -> None:
        toolbar = QToolBar("Tools", self)
        toolbar.setMovable(False)
        toolbar.addWidget(QLabel("Level "))
        toolbar.addWidget(self.level_combo)
        toolbar.addSeparator()
        toolbar.addWidget(QLabel("Tool "))
        toolbar.addWidget(self.mode_combo)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, toolbar)

    def set_map(self, map_data: dict, mark_dirty: bool) -> None:
        prior_selection: tuple[str, dict] | None = None
        if self.selected_spawn_zone_ref is not None:
            list_name, idx = self.selected_spawn_zone_ref
            if 0 <= idx < len(self.map_data[list_name]):
                prior_selection = (list_name, copy.deepcopy(self.map_data[list_name][idx]))
        self.map_data = canonicalize_map(map_data)
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        if prior_selection is not None:
            list_name, snapshot = prior_selection
            new_idx = self._find_zone_index(list_name, snapshot)
            self.selected_spawn_zone_ref = (list_name, new_idx) if new_idx is not None else None
        else:
            self.selected_spawn_zone_ref = None
        if mark_dirty:
            self.dirty = True
        self.refresh_ui()

    def apply_change(self, label: str, after: dict) -> None:
        before = self.map_data
        if canonicalize_map(before) == canonicalize_map(after):
            return
        self.undo_stack.push(SetMapCommand(self, label, before, after))

    def refresh_ui(self) -> None:
        self.level_combo.blockSignals(True)
        self.level_combo.clear()
        for idx, level in enumerate(self.map_data["levels"]):
            self.level_combo.addItem(level_label(level, idx))
        self.level_combo.setCurrentIndex(self.current_level)
        self.level_combo.blockSignals(False)
        self.canvas.update()
        self.update_status()
        suffix = "*" if self.dirty else ""
        file_name = str(self.path) if self.path else "Untitled"
        self.setWindowTitle(f"Cuboid Wars Editor - {file_name}{suffix}")

    def resize_to_map(self) -> None:
        self.canvas.updateGeometry()
        self.resize(self.sizeHint())

    def update_status(self) -> None:
        errors = validate_map(self.map_data)
        if errors:
            self.status_label.setText(f"{len(errors)} structural issue(s)")
            self.status_label.setToolTip("\n".join(errors[:20]))
        else:
            self.status_label.setText("Structurally valid")
            self.status_label.setToolTip("")

    def select_level(self, index: int) -> None:
        if 0 <= index < len(self.map_data["levels"]):
            self.current_level = index
            self.canvas.update()

    def set_mode(self, mode: str) -> None:
        self.mode = mode

    def previous_level(self) -> None:
        self.set_level_index(self.current_level - 1)

    def next_level(self) -> None:
        self.set_level_index(self.current_level + 1)

    def set_level_index(self, index: int) -> None:
        clamped = max(0, min(index, len(self.map_data["levels"]) - 1))
        if clamped == self.current_level:
            return
        self.current_level = clamped
        self.refresh_ui()

    def previous_tool(self) -> None:
        self.set_tool_index(self.mode_combo.currentIndex() - 1)

    def next_tool(self) -> None:
        self.set_tool_index(self.mode_combo.currentIndex() + 1)

    def set_tool_index(self, index: int) -> None:
        count = self.mode_combo.count()
        if count == 0:
            return
        self.mode_combo.setCurrentIndex(index % count)

    def open_file(self) -> None:
        if not self.confirm_discard_changes():
            return
        path, _ = QFileDialog.getOpenFileName(self, "Open Map", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        try:
            self.map_data = read_map(Path(path))
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        self.path = Path(path)
        self.current_level = 0
        self.dirty = False
        self.undo_stack.clear()
        self.refresh_ui()

    def save(self) -> None:
        if self.path is None:
            self.save_as()
            return
        errors = validate_map(self.map_data)
        if errors:
            QMessageBox.warning(self, "Cannot Save", "Fix structural issues before saving:\n\n" + "\n".join(errors[:12]))
            return
        try:
            write_map(self.path, self.map_data)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return
        self.dirty = False
        self.refresh_ui()

    def save_as(self) -> None:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        self.path = Path(path)
        self.save()

    def confirm_discard_changes(self) -> bool:
        if not self.dirty:
            return True
        result = QMessageBox.question(
            self,
            "Unsaved Changes",
            "Discard unsaved changes?",
            QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        return result == QMessageBox.StandardButton.Discard

    def add_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        floors = {tuple(f) for f in level["floors"]}
        inaccessible_floors = {tuple(f) for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                floors.add((col, row))
                inaccessible_floors.discard((col, row))
        level["floors"] = [[c, r] for c, r in floors]
        level["inaccessible_floors"] = [[c, r] for c, r in inaccessible_floors]
        self.apply_change("Paint Floor", after)

    def add_inaccessible_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        floors = {tuple(f) for f in level["floors"]}
        inaccessible_floors = {tuple(f) for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                floors.discard((col, row))
                inaccessible_floors.add((col, row))
        level["floors"] = [[c, r] for c, r in floors]
        level["inaccessible_floors"] = [[c, r] for c, r in inaccessible_floors]
        # Drop any spawn zone whose rect intersects the new inaccessible-floor rect on this level.
        for list_name in SPAWN_ZONE_LISTS:
            after[list_name] = [
                zone
                for zone in after[list_name]
                if not (zone["level"] == self.current_level and zone_intersects_rect(zone, (c0, r0, c1, r1)))
            ]
        self.apply_change("Paint Inaccessible Floor", after)

    def add_actor_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        spawns = self.prompt_for_actor_spawn_entries()
        if spawns is None:
            return
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "spawns": spawns,
        }
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Actor Spawn Zone", after)
        self.recent_actor_spawn_entries = [dict(entry) for entry in spawns]
        new_idx = self._find_zone_index(ACTOR_ZONE_LIST, new_zone)
        self.selected_spawn_zone_ref = (ACTOR_ZONE_LIST, new_idx) if new_idx is not None else None

    def add_player_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
        }
        after[PLAYER_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Player Spawn Zone", after)
        new_idx = self._find_zone_index(PLAYER_ZONE_LIST, new_zone)
        self.selected_spawn_zone_ref = (PLAYER_ZONE_LIST, new_idx) if new_idx is not None else None

    def prompt_for_actor_spawn_entries(self, current: list[dict] | None = None) -> list[dict] | None:
        suggestion = format_spawn_entries(
            current if current is not None else self.recent_actor_spawn_entries
        )
        text, ok = QInputDialog.getText(
            self,
            "Actor Spawn Zone Entries",
            "Comma-separated entries as `kind:count` (e.g. actor:3, sniper:1):",
            text=suggestion,
        )
        if not ok:
            return None
        spawns = parse_spawn_entries_input(text)
        if not spawns:
            QMessageBox.warning(self, "Actor Spawn Zone Entries", "At least one entry is required.")
            return None
        return spawns

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        edges = wall_segments_between(start, end)
        if not edges:
            return
        after = copy.deepcopy(self.map_data)
        walls = {tuple(normalized_wall(w)) for w in after["levels"][self.current_level]["walls"]}
        walls.update(tuple(w) for w in edges)
        after["levels"][self.current_level]["walls"] = [list(w) for w in walls]
        self.apply_change("Place Wall", after)

    def add_ramp(self, start_cell: tuple[int, int], end_cell: tuple[int, int], mode: str) -> None:
        start_point, end_point = ramp_points_from_cells(start_cell, end_cell)
        if mode == MODE_RAMP_UP:
            if self.current_level + 1 >= len(self.map_data["levels"]):
                self.statusBar().showMessage("Ramp not placed: Ramp (Up) needs an upper level", 4000)
                return
            lower_level = self.current_level
            low = start_point
            high = end_point
        else:
            if self.current_level == 0:
                self.statusBar().showMessage("Ramp not placed: Ramp (Down) needs a lower level", 4000)
                return
            lower_level = self.current_level - 1
            low = end_point
            high = start_point

        msg = ramp_error(
            low,
            high,
            lower_level,
            self.map_data["grid_cols"],
            self.map_data["grid_rows"],
            len(self.map_data["levels"]),
        )
        if msg:
            self.statusBar().showMessage(f"Ramp not placed: {msg}", 4000)
            return
        new_ramp = {"low": low, "high": high, "lower_level": lower_level}
        new_rect = ramp_rect(new_ramp)
        after = copy.deepcopy(self.map_data)
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if self.current_level not in (ramp["lower_level"], ramp["lower_level"] + 1)
            or not rects_overlap(new_rect, ramp_rect(ramp))
        ]
        after["ramps"].append(new_ramp)
        self.apply_change(f"Place {mode}", after)

    def erase_at(self, pos, cell_size: float, preserve_floors: bool) -> None:
        hit = self.hit_at(pos, cell_size)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if not preserve_floors:
            level["floors"] = [
                floor
                for floor in level["floors"]
                if not (c0 <= floor[0] < c1 and r0 <= floor[1] < r1)
            ]
            level["inaccessible_floors"] = [
                floor
                for floor in level["inaccessible_floors"]
                if not (c0 <= floor[0] < c1 and r0 <= floor[1] < r1)
            ]
        level["walls"] = [
            wall
            for wall in level["walls"]
            if not wall_overlaps_rect(wall, (c0, r0, c1, r1))
        ]
        for list_name in SPAWN_ZONE_LISTS:
            after[list_name] = [
                zone
                for zone in after[list_name]
                if not (zone["level"] == self.current_level and zone_intersects_rect(zone, (c0, r0, c1, r1)))
            ]
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if self.current_level not in (ramp["lower_level"], ramp["lower_level"] + 1)
            or not rects_overlap((c0, r0, c1, r1), ramp_rect(ramp))
        ]
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(label, after)

    def hit_at(self, pos, cell_size: float):
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        level = self.map_data["levels"][self.current_level]
        px = pos.x() / cell_size
        py = pos.y() / cell_size

        for wall in level["walls"]:
            if point_near_wall(px, py, wall):
                return ("Wall", tuple(wall))
        # Search both lists in reverse so the most-recently-painted (visually
        # on top) wins. Actor first, then player — when both cover the same
        # cell, prefer the actor zone since its label needs editing more often.
        for list_name in (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST):
            for idx in range(len(self.map_data[list_name]) - 1, -1, -1):
                zone = self.map_data[list_name][idx]
                if zone["level"] == self.current_level and zone_contains_cell(zone, col, row):
                    return ("Spawn Zone", (list_name, idx))
        for ramp in self.map_data["ramps"]:
            lower = ramp["lower_level"]
            if self.current_level not in (lower, lower + 1):
                continue
            c0, r0, c1, r1 = ramp_rect(ramp)
            if c0 <= col < c1 and r0 <= row < r1:
                return ("Ramp", (lower, tuple(ramp["low"]), tuple(ramp["high"])))
        if [col, row] in level["floors"]:
            return ("Floor", (col, row))
        if [col, row] in level["inaccessible_floors"]:
            return ("Inaccessible Floor", (col, row))
        return None

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        kind, value = hit
        if preserve_floors and kind in FLOOR_HIT_KINDS:
            return
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if kind == "Floor":
            level["floors"] = [floor for floor in level["floors"] if tuple(floor) != value]
        elif kind == "Inaccessible Floor":
            level["inaccessible_floors"] = [floor for floor in level["inaccessible_floors"] if tuple(floor) != value]
        elif kind == "Spawn Zone":
            list_name, target_idx = value
            if 0 <= target_idx < len(after[list_name]):
                del after[list_name][target_idx]
                if self.selected_spawn_zone_ref == (list_name, target_idx):
                    self.selected_spawn_zone_ref = None
        elif kind == "Wall":
            level["walls"] = [wall for wall in level["walls"] if tuple(normalized_wall(wall)) != value]
        elif kind == "Ramp":
            lower, low, high = value
            after["ramps"] = [
                ramp
                for ramp in after["ramps"]
                if (ramp["lower_level"], tuple(ramp["low"]), tuple(ramp["high"])) != (lower, low, high)
            ]
        self.apply_change(f"Erase {kind}", after)

    def add_level(self) -> None:
        after = copy.deepcopy(self.map_data)
        insert_at = self.current_level + 1
        after["levels"].insert(
            insert_at,
            {"name": f"Level {insert_at}", "floors": [], "inaccessible_floors": [], "walls": []},
        )
        for list_name in SPAWN_ZONE_LISTS:
            for zone in after[list_name]:
                if zone["level"] >= insert_at:
                    zone["level"] += 1
        for ramp in after["ramps"]:
            if ramp["lower_level"] >= insert_at:
                ramp["lower_level"] += 1
        self.apply_change("Add Level", after)
        self.current_level = insert_at
        self.refresh_ui()

    def rename_level(self) -> None:
        level = self.map_data["levels"][self.current_level]
        text, ok = QInputDialog.getText(self, "Rename Level", "Name:", text=level.get("name") or "")
        if not ok:
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][self.current_level]["name"] = text.strip() or f"Level {self.current_level}"
        self.apply_change("Rename Level", after)

    def remove_level(self) -> None:
        if len(self.map_data["levels"]) == 1:
            QMessageBox.information(self, "Remove Level", "A map must have at least one level.")
            return
        result = QMessageBox.question(
            self,
            "Remove Level",
            f"Remove {level_label(self.map_data['levels'][self.current_level], self.current_level)}?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        if result != QMessageBox.StandardButton.Yes:
            return
        removed = self.current_level
        after = copy.deepcopy(self.map_data)
        after["levels"].pop(removed)
        for list_name in SPAWN_ZONE_LISTS:
            adjusted_zones = []
            for zone in after[list_name]:
                if zone["level"] == removed:
                    continue
                if zone["level"] > removed:
                    zone["level"] -= 1
                adjusted_zones.append(zone)
            after[list_name] = adjusted_zones
        adjusted = []
        for ramp in after["ramps"]:
            lower = ramp["lower_level"]
            upper = lower + 1
            if removed in (lower, upper):
                continue
            if lower > removed:
                ramp["lower_level"] = lower - 1
            adjusted.append(ramp)
        after["ramps"] = adjusted
        self.current_level = max(0, min(removed, len(after["levels"]) - 1))
        self.apply_change("Remove Level", after)

    def show_tool_reference(self) -> None:
        QMessageBox.information(
            self,
            "Tool Reference",
            "Floor: drag cells to add floor.\n"
            "Inaccessible Floor: drag cells to add floor slabs that never spawn items, players, or lights.\n"
            "Actor Spawn Zone (Paint): drag a rectangle, then enter `kind:count` entries (e.g. actor:3).\n"
            "Player Spawn Zone (Paint): drag a rectangle. No prompt — players spawn anywhere in any player zone.\n"
            "Spawn Zone (Edit): click a zone to select; drag the body to move, drag a corner/edge handle to resize. Right-click to edit entries (actor zones only) or delete.\n"
            "Wall: drag along grid lines to place atomic wall edges.\n"
            "Ramp (Up): drag from this level toward the upper level.\n"
            "Ramp (Down): drag from this level toward the lower level.\n"
            "Erase: click an item, drag cells to erase an area, or right-click for the context menu.\n"
            "Erase (Keep Floors): erase walls, ramps, and spawn zones while preserving floor and inaccessible floor cells.",
        )

    # ============================================================================
    # Spawn zone edit-mode helpers
    # ============================================================================

    def selected_spawn_zone(self) -> dict | None:
        ref = self.selected_spawn_zone_ref
        if ref is None:
            return None
        list_name, idx = ref
        if not (0 <= idx < len(self.map_data[list_name])):
            return None
        return self.map_data[list_name][idx]

    def set_selected_spawn_zone(self, ref: tuple[str, int] | None) -> None:
        if ref is None:
            self.selected_spawn_zone_ref = None
        else:
            list_name, idx = ref
            if list_name in SPAWN_ZONE_LISTS and 0 <= idx < len(self.map_data[list_name]):
                self.selected_spawn_zone_ref = (list_name, idx)
            else:
                self.selected_spawn_zone_ref = None
        self.canvas.update()

    def _find_zone_index(self, list_name: str, target: dict) -> int | None:
        key = zone_key(list_name, target)
        for idx, zone in enumerate(self.map_data[list_name]):
            if zone_key(list_name, zone) == key:
                return idx
        return None

    def spawn_zone_at(self, pos, cell_size: float) -> tuple[str, int] | None:
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        # Iterate in reverse so the most-recently-painted wins. Actor zones
        # take priority over player zones when both cover the cell — author
        # is more likely to want to edit the actor entries.
        for list_name in (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST):
            for idx in range(len(self.map_data[list_name]) - 1, -1, -1):
                zone = self.map_data[list_name][idx]
                if zone["level"] == self.current_level and zone_contains_cell(zone, col, row):
                    return (list_name, idx)
        return None

    def begin_spawn_zone_edit_press(self, pos, cell_size: float) -> None:
        # Try a handle on the currently selected zone first.
        zone = self.selected_spawn_zone()
        if zone is not None and zone["level"] == self.current_level:
            handle = self._handle_at_pos(zone, pos, cell_size)
            if handle is not None:
                assert self.selected_spawn_zone_ref is not None
                list_name, idx = self.selected_spawn_zone_ref
                self.spawn_zone_drag = (
                    list_name,
                    idx,
                    handle,
                    (pos.x() / cell_size, pos.y() / cell_size),
                    copy.deepcopy(zone),
                )
                return
        # Otherwise pick the zone under the cursor.
        ref = self.spawn_zone_at(pos, cell_size)
        if ref is None:
            self.set_selected_spawn_zone(None)
            self.spawn_zone_drag = None
            return
        self.set_selected_spawn_zone(ref)
        list_name, idx = ref
        self.spawn_zone_drag = (
            list_name,
            idx,
            "move",
            (pos.x() / cell_size, pos.y() / cell_size),
            copy.deepcopy(self.map_data[list_name][idx]),
        )

    def _handle_at_pos(self, zone: dict, pos, cell_size: float) -> str | None:
        handle_names = ["nw", "n", "ne", "e", "se", "s", "sw", "w"]
        centers = self.canvas.spawn_zone_handle_centers(zone, cell_size)
        radius = max(SPAWN_ZONE_HANDLE_PIXELS * 0.75, 6.0)
        for name, (cx, cy) in zip(handle_names, centers):
            if abs(pos.x() - cx) <= radius and abs(pos.y() - cy) <= radius:
                return name
        return None

    def update_spawn_zone_edit_drag(self, pos, cell_size: float) -> None:
        if self.spawn_zone_drag is None:
            return
        self._drag_current_cell_pos = (pos.x() / cell_size, pos.y() / cell_size)

    def spawn_zone_candidate_rect(self) -> tuple[int, int, int, int] | None:
        if self.spawn_zone_drag is None or not hasattr(self, "_drag_current_cell_pos"):
            return None
        _, _, mode, origin, original = self.spawn_zone_drag
        ox, oy = origin
        cx, cy = self._drag_current_cell_pos
        dx_cells = round(cx - ox)
        dy_cells = round(cy - oy)
        c0, r0, c1, r1 = zone_rect(original)
        cols_max = self.map_data["grid_cols"]
        rows_max = self.map_data["grid_rows"]
        if mode == "move":
            new_c0 = max(0, min(cols_max - (c1 - c0), c0 + dx_cells))
            new_r0 = max(0, min(rows_max - (r1 - r0), r0 + dy_cells))
            return (new_c0, new_r0, new_c0 + (c1 - c0), new_r0 + (r1 - r0))
        new_c0, new_r0, new_c1, new_r1 = c0, r0, c1, r1
        if "n" in mode:
            new_r0 = max(0, min(r1 - 1, r0 + dy_cells))
        if "s" in mode:
            new_r1 = max(r0 + 1, min(rows_max, r1 + dy_cells))
        if "w" in mode:
            new_c0 = max(0, min(c1 - 1, c0 + dx_cells))
        if "e" in mode:
            new_c1 = max(c0 + 1, min(cols_max, c1 + dx_cells))
        return (new_c0, new_r0, new_c1, new_r1)

    def commit_spawn_zone_edit_drag(self) -> None:
        if self.spawn_zone_drag is None:
            return
        candidate = self.spawn_zone_candidate_rect()
        list_name, zone_idx, _mode, _origin, original = self.spawn_zone_drag
        self.spawn_zone_drag = None
        if hasattr(self, "_drag_current_cell_pos"):
            del self._drag_current_cell_pos
        if candidate is None:
            return
        c0, r0, c1, r1 = candidate
        if (c0, r0, c1, r1) == zone_rect(original):
            return
        if not (0 <= zone_idx < len(self.map_data[list_name])):
            return
        after = copy.deepcopy(self.map_data)
        zone = after[list_name][zone_idx]
        zone["cols"] = [c0, c1]
        zone["rows"] = [r0, r1]
        self.apply_change("Edit Spawn Zone", after)
        new_idx = self._find_zone_index(list_name, zone)
        self.selected_spawn_zone_ref = (list_name, new_idx) if new_idx is not None else None

    def edit_selected_spawn_zone_spawns(self) -> None:
        zone = self.selected_spawn_zone()
        if zone is None or self.selected_spawn_zone_ref is None:
            return
        list_name, zone_idx = self.selected_spawn_zone_ref
        if list_name != ACTOR_ZONE_LIST:
            return  # Player zones have no spawns list.
        spawns = self.prompt_for_actor_spawn_entries(zone["spawns"])
        if spawns is None:
            return
        after = copy.deepcopy(self.map_data)
        if not (0 <= zone_idx < len(after[list_name])):
            return
        after[list_name][zone_idx]["spawns"] = spawns
        self.apply_change("Edit Spawn Zone Entries", after)
        self.recent_actor_spawn_entries = [dict(entry) for entry in spawns]
        new_idx = self._find_zone_index(list_name, after[list_name][zone_idx])
        self.selected_spawn_zone_ref = (list_name, new_idx) if new_idx is not None else None

    def delete_selected_spawn_zone(self) -> None:
        ref = self.selected_spawn_zone_ref
        if ref is None:
            return
        list_name, zone_idx = ref
        if not (0 <= zone_idx < len(self.map_data[list_name])):
            return
        after = copy.deepcopy(self.map_data)
        del after[list_name][zone_idx]
        self.selected_spawn_zone_ref = None
        self.apply_change("Delete Spawn Zone", after)

    def closeEvent(self, event) -> None:
        if self.confirm_discard_changes():
            event.accept()
        else:
            event.ignore()


def rect_from_cells(a: tuple[int, int], b: tuple[int, int]) -> tuple[int, int, int, int]:
    c0 = min(a[0], b[0])
    r0 = min(a[1], b[1])
    c1 = max(a[0], b[0]) + 1
    r1 = max(a[1], b[1]) + 1
    return c0, r0, c1, r1


def ramp_points_from_cells(start: tuple[int, int], end: tuple[int, int]) -> tuple[list[int], list[int]]:
    c0, r0, c1, r1 = rect_from_cells(start, end)
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) >= abs(dy):
        if dx >= 0:
            return [c0, r0], [c1, r1]
        return [c1, r0], [c0, r1]
    if dy >= 0:
        return [c0, r0], [c1, r1]
    return [c0, r1], [c1, r0]


def rects_overlap(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> bool:
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def wall_overlaps_rect(wall: list[int], rect: tuple[int, int, int, int]) -> bool:
    c0, r0, c1, r1 = rect
    wc0, wr0, wc1, wr1 = wall
    if wr0 == wr1:
        left = min(wc0, wc1)
        right = max(wc0, wc1)
        return r0 <= wr0 <= r1 and left < c1 and c0 < right
    top = min(wr0, wr1)
    bottom = max(wr0, wr1)
    return c0 <= wc0 <= c1 and top < r1 and r0 < bottom


def snapped_wall_end(start: tuple[int, int], current: tuple[int, int]) -> tuple[int, int]:
    dx = current[0] - start[0]
    dy = current[1] - start[1]
    if abs(dx) >= abs(dy):
        return current[0], start[1]
    return start[0], current[1]


def draw_direction(start: tuple[int, int], end: tuple[int, int]) -> str:
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def orthogonal_arrow_points(
    c0: int,
    r0: int,
    c1: int,
    r1: int,
    direction: str,
    cell: float,
) -> tuple[tuple[float, float], tuple[float, float]]:
    pad = min(cell * 0.35, 14.0)
    left = c0 * cell + pad
    right = c1 * cell - pad
    top = r0 * cell + pad
    bottom = r1 * cell - pad
    mid_x = (c0 + c1) * cell / 2.0
    mid_y = (r0 + r1) * cell / 2.0
    if direction == "east":
        return (left, mid_y), (right, mid_y)
    if direction == "west":
        return (right, mid_y), (left, mid_y)
    if direction == "south":
        return (mid_x, top), (mid_x, bottom)
    return (mid_x, bottom), (mid_x, top)


def wall_segments_between(start: tuple[int, int], end: tuple[int, int]) -> list[list[int]]:
    if start == end:
        return []
    c0, r0 = start
    c1, r1 = end
    edges = []
    if r0 == r1:
        step = 1 if c1 > c0 else -1
        for col in range(c0, c1, step):
            edges.append(normalized_wall([col, r0, col + step, r0]))
    elif c0 == c1:
        step = 1 if r1 > r0 else -1
        for row in range(r0, r1, step):
            edges.append(normalized_wall([c0, row, c0, row + step]))
    return edges


def point_near_wall(px: float, py: float, wall: list[int], tolerance: float = 0.16) -> bool:
    c0, r0, c1, r1 = wall
    if r0 == r1:
        return min(c0, c1) - tolerance <= px <= max(c0, c1) + tolerance and abs(py - r0) <= tolerance
    return min(r0, r1) - tolerance <= py <= max(r0, r1) + tolerance and abs(px - c0) <= tolerance


def main() -> int:
    parser = argparse.ArgumentParser(description="Cuboid Wars map editor.")
    parser.add_argument("file", nargs="?", type=Path, default=DEFAULT_MAP, help="Map JSON to edit.")
    args = parser.parse_args()

    app = QApplication(sys.argv)
    signal.signal(signal.SIGINT, lambda _signum, _frame: app.exit(130))
    sigint_timer = QTimer()
    sigint_timer.timeout.connect(lambda: None)
    sigint_timer.start(100)

    window = EditorWindow(args.file)
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())

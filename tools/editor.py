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
from dataclasses import dataclass

from PySide6.QtCore import QPoint, QRectF, QSize, Qt, QTimer
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
    QButtonGroup,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QGridLayout,
    QGroupBox,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMainWindow,
    QMenu,
    QMessageBox,
    QSizePolicy,
    QSpinBox,
    QToolBar,
    QToolButton,
    QVBoxLayout,
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
MODE_FLOOR_MATERIAL = "Floor Material"
MODE_WALL_MATERIAL = "Wall Material"
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
    MODE_FLOOR_MATERIAL,
    MODE_WALL_MATERIAL,
]

# Two named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
PLAYER_ZONE_LIST = "player_spawn_zones"
SPAWN_ZONE_LISTS = (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST)

DEFAULT_ACTOR_COUNT = 1
SPAWN_ZONE_HANDLE_PIXELS = 8.0


@dataclass
class ZoneRef:
    """Identifies a spawn zone by which list it belongs to and its index."""

    list_name: str
    index: int


@dataclass
class SpawnZoneDrag:
    """In-flight spawn-zone resize/move state used by Spawn Zone (Edit) mode."""

    list_name: str
    index: int
    handle: str  # "move" or one of "n"/"s"/"e"/"w"/"nw"/"ne"/"sw"/"se"
    origin: tuple[float, float]  # cursor position when drag started, in cell coords
    original_zone: dict  # snapshot of the zone before the drag


# ============================================================================
# Modal dialogs
# ============================================================================


class ActorSpawnFieldsDialog(QDialog):
    """Modal dialog with labeled Kind (text) + Count (integer spinbox) fields.

    Used both when painting a new actor zone and when editing an existing
    one. Returns (kind, count) on accept; None on cancel.
    """

    MAX_COUNT = 9999

    def __init__(self, parent, kind: str, count: int):
        super().__init__(parent)
        self.setWindowTitle("Actor Spawn Zone")

        self._kind_edit = QLineEdit(kind)
        self._count_spin = QSpinBox()
        self._count_spin.setRange(0, self.MAX_COUNT)
        self._count_spin.setValue(count)

        form = QFormLayout()
        form.addRow("Kind:", self._kind_edit)
        form.addRow("Count:", self._count_spin)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(buttons)

    def values(self) -> tuple[str, int]:
        return self._kind_edit.text().strip(), self._count_spin.value()

    @classmethod
    def prompt(cls, parent, kind: str, count: int) -> tuple[str, int] | None:
        dialog = cls(parent, kind, count)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        new_kind, new_count = dialog.values()
        if not new_kind:
            QMessageBox.warning(parent, "Actor Spawn Zone", "Kind is required.")
            return None
        return new_kind, new_count


class ResizeMapDialog(QDialog):
    """Modal dialog to resize the map.

    Lets the user pick new column/row counts and an anchor — a 3x3 grid of
    radio buttons indicating where the existing content stays in the new
    canvas (top-left, center, etc.). Returns (new_cols, new_rows, anchor_x,
    anchor_y) on accept; None on cancel.
    """

    MIN_DIM = 1
    MAX_DIM = 256

    def __init__(self, parent, current_cols: int, current_rows: int):
        super().__init__(parent)
        self.setWindowTitle("Resize Map")

        self._cols_spin = QSpinBox()
        self._cols_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._cols_spin.setValue(current_cols)
        self._rows_spin = QSpinBox()
        self._rows_spin.setRange(self.MIN_DIM, self.MAX_DIM)
        self._rows_spin.setValue(current_rows)

        form = QFormLayout()
        form.addRow("Current size:", QLabel(f"{current_cols} × {current_rows}"))
        form.addRow("New columns:", self._cols_spin)
        form.addRow("New rows:", self._rows_spin)

        anchor_box = QGroupBox("Anchor (where existing content stays)")
        anchor_grid = QGridLayout(anchor_box)
        anchor_grid.setSpacing(2)
        self._anchor_group = QButtonGroup(self)
        self._anchor_group.setExclusive(True)
        labels = [
            ["↖", "↑", "↗"],
            ["←", "•", "→"],
            ["↙", "↓", "↘"],
        ]
        for ay in range(3):
            for ax in range(3):
                button = QToolButton()
                button.setCheckable(True)
                button.setText(labels[ay][ax])
                button.setFixedSize(32, 32)
                anchor_id = ay * 3 + ax
                self._anchor_group.addButton(button, anchor_id)
                anchor_grid.addWidget(button, ay, ax)
        self._anchor_group.button(1 * 3 + 1).setChecked(True)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(anchor_box)
        layout.addWidget(buttons)

    def values(self) -> tuple[int, int, int, int]:
        anchor_id = self._anchor_group.checkedId()
        anchor_x = anchor_id % 3
        anchor_y = anchor_id // 3
        return self._cols_spin.value(), self._rows_spin.value(), anchor_x, anchor_y

    @classmethod
    def prompt(cls, parent, current_cols: int, current_rows: int) -> tuple[int, int, int, int] | None:
        dialog = cls(parent, current_cols, current_rows)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()


class MaterialAssignmentDialog(QDialog):
    """Modal dialog with one dropdown per face (top/bottom/N/S/E/W).

    `catalog` is the list of material names to choose from, sourced from
    `assets.json`. `initial` provides the starting selection per face;
    typically the materials of the first selected segment, so opening on a
    uniform region pre-fills with current values.

    `Apply to all` copies the Top dropdown's value into the other five.
    """

    FACE_LABELS = (("top", "Top"), ("bottom", "Bottom"), ("north", "North"), ("south", "South"), ("east", "East"), ("west", "West"))

    def __init__(self, parent, title: str, scope_summary: str, catalog: list[str], initial: dict[str, str]):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._dropdowns: dict[str, QComboBox] = {}
        form = QFormLayout()
        form.addRow("Selection:", QLabel(scope_summary))
        for face, label in self.FACE_LABELS:
            combo = QComboBox()
            combo.addItems(catalog)
            current = initial.get(face)
            if current is not None and current in catalog:
                combo.setCurrentText(current)
            self._dropdowns[face] = combo
            form.addRow(label + ":", combo)

        apply_all_button = QToolButton()
        apply_all_button.setText("Apply Top to all faces")
        apply_all_button.clicked.connect(self._apply_top_to_all)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(apply_all_button)
        layout.addWidget(buttons)

    def _apply_top_to_all(self) -> None:
        top_value = self._dropdowns["top"].currentText()
        for face in ("bottom", "north", "south", "east", "west"):
            self._dropdowns[face].setCurrentText(top_value)

    def values(self) -> dict[str, str]:
        return {face: self._dropdowns[face].currentText() for face, _ in self.FACE_LABELS}

    @classmethod
    def prompt(
        cls,
        parent,
        title: str,
        scope_summary: str,
        catalog: list[str],
        initial: dict[str, str],
    ) -> dict[str, str] | None:
        if not catalog:
            QMessageBox.warning(parent, title, "No materials catalog loaded (assets.json missing or empty).")
            return None
        dialog = cls(parent, title, scope_summary, catalog, initial)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return None
        return dialog.values()


MIN_CELL = 12.0
EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20


# ============================================================================
# Map schema
#
# The on-disk schema lives in `config/server/map.json`. In memory, every
# floor / wall / ramp record is a dict carrying its grid coordinates and
# six face materials (top / bottom / north / south / east / west).
#
# Reading flow:        read_map -> normalize_map -> canonicalize_map
# Writing flow:        write_map -> canonicalize_map -> format_map_file
# Editing transforms:  resize_map_data, validate_map
# ============================================================================


def empty_map() -> dict:
    # No seeded actor zone: there's no default kind to give it. Users paint
    # actor zones explicitly and pick a kind in the dialog.
    return {
        "grid_cols": DEFAULT_GRID_COLS,
        "grid_rows": DEFAULT_GRID_ROWS,
        "actor_spawn_zones": [],
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


def load_materials_catalog(map_path: Path | None) -> list[str]:
    """Return the sorted list of material names defined in the project's
    `assets.json`. Returns an empty list if the file can't be located —
    callers handle that gracefully."""
    candidates: list[Path] = []
    if map_path is not None:
        # config/server/map.json -> config/client/assets.json
        candidates.append(map_path.parent.parent / "client" / "assets.json")
    # Fallback: relative to the editor's repo layout.
    candidates.append(Path(__file__).resolve().parent.parent / "config" / "client" / "assets.json")
    for candidate in candidates:
        if candidate.exists():
            try:
                with candidate.open("r", encoding="utf-8") as handle:
                    assets = json.load(handle)
                materials = assets.get("materials") or {}
                return sorted(materials.keys())
            except (OSError, json.JSONDecodeError):
                continue
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


def json_scalar(value) -> str:
    return json.dumps(value, separators=(",", ": "))


def format_point(point: list[int]) -> str:
    return "[" + ", ".join(str(v) for v in point) + "]"


def with_trailing_comma(lines: list[str]) -> list[str]:
    if lines:
        lines[-1] += ","
    return lines


def format_ramp(ramp: dict, indent: int) -> str:
    pad = " " * indent
    materials = compact_face_materials(ramp)
    materials_part = ""
    if materials:
        materials_part = ", " + ", ".join(
            f'"{key}": {json.dumps(value)}' for key, value in materials.items()
        )
    return (
        f'{pad}{{"lower_level": {ramp["lower_level"]}, '
        f'"low": {format_point(ramp["low"])}, '
        f'"high": {format_point(ramp["high"])}{materials_part}}}'
    )


def _zone_rect_fragment(zone: dict) -> str:
    return (
        f'"level": {zone["level"]}, '
        f'"cols": [{zone["cols"][0]}, {zone["cols"][1]}], '
        f'"rows": [{zone["rows"][0]}, {zone["rows"][1]}]'
    )


def format_actor_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    def render(zone: dict) -> str:
        return (
            f"{{{_zone_rect_fragment(zone)}, "
            f'"kind": {json.dumps(zone["kind"])}, '
            f'"count": {zone["count"]}}}'
        )

    return _format_zone_list("actor_spawn_zones", zones, indent, render)


def format_player_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    return _format_zone_list(
        "player_spawn_zones",
        zones,
        indent,
        lambda zone: f"{{{_zone_rect_fragment(zone)}}}",
    )


def _format_zone_list(name: str, zones: list[dict], indent: int, render_zone) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not zones:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    for idx, zone in enumerate(zones):
        comma = "," if idx + 1 < len(zones) else ""
        lines.append(f"{inner}{render_zone(zone)}{comma}")
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
                *with_trailing_comma(format_floor_array("floors", level["floors"], 8)),
                *with_trailing_comma(format_floor_array("inaccessible_floors", level["inaccessible_floors"], 8)),
                *format_wall_array("walls", level["walls"], 8),
                "      }" + ("," if level_idx + 1 < len(map_data["levels"]) else ""),
            ]
        )

    lines.append("    ],")
    item_materials = map_data.get("item_materials") or {}
    has_items = bool(item_materials)
    ramp_trailing_comma = "," if has_items else ""
    if map_data["ramps"]:
        lines.append('    "ramps": [')
        for idx, ramp in enumerate(map_data["ramps"]):
            comma = "," if idx + 1 < len(map_data["ramps"]) else ""
            lines.append(format_ramp(ramp, 6) + comma)
        lines.append(f"    ]{ramp_trailing_comma}")
    else:
        lines.append(f'    "ramps": []{ramp_trailing_comma}')
    if has_items:
        lines.append('    "item_materials": ' + json.dumps(item_materials, separators=(", ", ": "), sort_keys=True))
    lines.append("  }")
    lines.append("}")
    return "\n".join(lines)


def format_floor_array(name: str, floors: list[dict], indent: int) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not floors:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    for idx, floor in enumerate(floors):
        comma = "," if idx + 1 < len(floors) else ""
        body = {"col": floor["col"], "row": floor["row"], **compact_face_materials(floor)}
        lines.append(inner + json.dumps(body, separators=(", ", ": ")) + comma)
    lines.append(f"{pad}]")
    return lines


def format_wall_array(name: str, walls: list[dict], indent: int) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not walls:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    for idx, wall in enumerate(walls):
        comma = "," if idx + 1 < len(walls) else ""
        body = {
            "c0": wall["c0"], "r0": wall["r0"], "c1": wall["c1"], "r1": wall["r1"],
            **compact_face_materials(wall),
        }
        lines.append(inner + json.dumps(body, separators=(", ", ": ")) + comma)
    lines.append(f"{pad}]")
    return lines


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
                "floors": [normalize_floor(f) for f in level.get("floors", [])],
                "inaccessible_floors": [normalize_floor(f) for f in level.get("inaccessible_floors", [])],
                "walls": [normalize_wall(w) for w in level.get("walls", [])],
            }
        )
    if not levels:
        levels = [{"name": "Level 0", "floors": [], "inaccessible_floors": [], "walls": []}]

    ramps = [normalize_ramp(r) for r in map_data.get("ramps", [])]
    item_materials = {
        str(k): str(v) for k, v in map_data.get("item_materials", {}).items()
    }
    return {
        "grid_cols": cols,
        "grid_rows": rows,
        "actor_spawn_zones": actor_spawn_zones,
        "player_spawn_zones": player_spawn_zones,
        "levels": levels,
        "ramps": ramps,
        "item_materials": item_materials,
    }


def normalize_floor(floor: dict) -> dict:
    return {
        "col": int(floor["col"]),
        "row": int(floor["row"]),
        **expand_face_materials(floor),
    }


def normalize_wall(wall: dict) -> dict:
    return {
        "c0": int(wall["c0"]),
        "r0": int(wall["r0"]),
        "c1": int(wall["c1"]),
        "r1": int(wall["r1"]),
        **expand_face_materials(wall),
    }


def normalize_ramp(ramp: dict) -> dict:
    return {
        "low": [int(ramp["low"][0]), int(ramp["low"][1])],
        "high": [int(ramp["high"][0]), int(ramp["high"][1])],
        "lower_level": int(ramp["lower_level"]),
        **expand_face_materials(ramp),
    }


def _normalize_zone_rect(zone: dict) -> dict:
    cols = zone.get("cols") or [0, 0]
    rows = zone.get("rows") or [0, 0]
    return {
        "level": int(zone.get("level", 0)),
        "cols": [int(cols[0]), int(cols[1])],
        "rows": [int(rows[0]), int(rows[1])],
    }


def normalize_actor_spawn_zone(zone: dict) -> dict:
    kind = str(zone.get("kind", ""))
    try:
        count = int(zone.get("count", 0))
    except (TypeError, ValueError):
        count = 0
    return {**_normalize_zone_rect(zone), "kind": kind, "count": max(0, count)}


def normalize_player_spawn_zone(zone: dict) -> dict:
    return _normalize_zone_rect(zone)


def actor_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
        zone["kind"],
        zone["count"],
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
    b["actor_spawn_zones"] = _dedupe_sorted(b["actor_spawn_zones"], actor_zone_key)
    b["player_spawn_zones"] = _dedupe_sorted(b["player_spawn_zones"], player_zone_key)
    for level in b["levels"]:
        # Dedupe by (col, row); later entries win when the same position is
        # painted twice, so the user's most recent paint stays.
        level["floors"] = _dedupe_floors(level["floors"])
        floor_keys = {(f["col"], f["row"]) for f in level["floors"]}
        level["inaccessible_floors"] = [
            f for f in _dedupe_floors(level["inaccessible_floors"])
            if (f["col"], f["row"]) not in floor_keys
        ]
        level["walls"] = _dedupe_walls(level["walls"])

    b["ramps"] = sorted(
        b["ramps"],
        key=lambda r: (r["lower_level"], tuple(r["low"]), tuple(r["high"])),
    )
    return b


def _dedupe_floors(floors: list[dict]) -> list[dict]:
    by_pos: dict[tuple[int, int], dict] = {}
    for floor in floors:
        by_pos[(floor["col"], floor["row"])] = floor
    return [by_pos[k] for k in sorted(by_pos.keys(), key=lambda p: (p[1], p[0]))]


def _dedupe_walls(walls: list[dict]) -> list[dict]:
    by_edge: dict[tuple[int, int, int, int], dict] = {}
    for wall in walls:
        c0, r0, c1, r1 = normalized_wall([wall["c0"], wall["r0"], wall["c1"], wall["r1"]])
        by_edge[(c0, r0, c1, r1)] = {**wall, "c0": c0, "r0": r0, "c1": c1, "r1": r1}
    return [by_edge[k] for k in sorted(by_edge.keys())]


def resize_map_data(
    map_data: dict, new_cols: int, new_rows: int, anchor_x: int, anchor_y: int
) -> dict:
    """Translate and clip every coordinate in a map to fit a new grid size.

    `anchor_x` and `anchor_y` are each one of {0, 1, 2} indicating where the
    old grid sits inside the new grid (0=left/top, 1=center, 2=right/bottom).
    Cells, wall endpoints, and ramp endpoints are translated by the resulting
    offset; anything that falls outside the new bounds is dropped. Materials
    ride along with each segment.
    """
    old_cols = map_data["grid_cols"]
    old_rows = map_data["grid_rows"]
    dc = (new_cols - old_cols) * anchor_x // 2
    dr = (new_rows - old_rows) * anchor_y // 2

    out = copy.deepcopy(map_data)
    out["grid_cols"] = new_cols
    out["grid_rows"] = new_rows

    def cell_in_bounds(c: int, r: int) -> bool:
        return 0 <= c < new_cols and 0 <= r < new_rows

    def line_in_bounds(c: int, r: int) -> bool:
        return 0 <= c <= new_cols and 0 <= r <= new_rows

    def shift_floor(f: dict) -> dict | None:
        nc, nr = f["col"] + dc, f["row"] + dr
        if not cell_in_bounds(nc, nr):
            return None
        return {**f, "col": nc, "row": nr}

    def shift_wall(w: dict) -> dict | None:
        nc0, nr0 = w["c0"] + dc, w["r0"] + dr
        nc1, nr1 = w["c1"] + dc, w["r1"] + dr
        if not (line_in_bounds(nc0, nr0) and line_in_bounds(nc1, nr1)):
            return None
        return {**w, "c0": nc0, "r0": nr0, "c1": nc1, "r1": nr1}

    for level in out["levels"]:
        level["floors"] = [f for f in (shift_floor(f) for f in level["floors"]) if f is not None]
        level["inaccessible_floors"] = [
            f for f in (shift_floor(f) for f in level["inaccessible_floors"]) if f is not None
        ]
        level["walls"] = [w for w in (shift_wall(w) for w in level["walls"]) if w is not None]

    def clip_zone(zone: dict) -> dict | None:
        c0, c1 = zone["cols"]
        r0, r1 = zone["rows"]
        nc0 = max(0, c0 + dc)
        nc1 = min(new_cols, c1 + dc)
        nr0 = max(0, r0 + dr)
        nr1 = min(new_rows, r1 + dr)
        if nc1 <= nc0 or nr1 <= nr0:
            return None
        zone["cols"] = [nc0, nc1]
        zone["rows"] = [nr0, nr1]
        return zone

    out["actor_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["actor_spawn_zones"]) if z is not None
    ]
    out["player_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["player_spawn_zones"]) if z is not None
    ]

    kept_ramps = []
    for ramp in out["ramps"]:
        low_c, low_r = ramp["low"][0] + dc, ramp["low"][1] + dr
        high_c, high_r = ramp["high"][0] + dc, ramp["high"][1] + dr
        if line_in_bounds(low_c, low_r) and line_in_bounds(high_c, high_r):
            ramp["low"] = [low_c, low_r]
            ramp["high"] = [high_c, high_r]
            kept_ramps.append(ramp)
    out["ramps"] = kept_ramps

    return out


def enforce_ramp_floor_rules(map_data: dict) -> None:
    # Mutates `map_data` in place. Only called from `canonicalize_map` after a
    # `deepcopy`, so the mutation is safe.
    for ramp in map_data["ramps"]:
        lower = ramp["lower_level"]
        upper = lower + 1
        if lower < 0 or upper >= len(map_data["levels"]):
            continue
        cells = ramp_cells(ramp)
        if not cells:
            continue

        # Ensure the ramp's footprint cells exist as regular floors on the
        # lower level (auto-painted with placeholder materials when missing),
        # and are removed from the upper level. Inaccessible-floor entries
        # at those cells are also dropped.
        ramp_faces = {face: ramp.get(face, "fiberous-plaster1-ue") for face in FACES}
        lower_existing = {(f["col"], f["row"]): f for f in map_data["levels"][lower]["floors"]}
        for col, row in cells:
            if (col, row) not in lower_existing:
                lower_existing[(col, row)] = {"col": col, "row": row, **ramp_faces}
        map_data["levels"][lower]["floors"] = list(lower_existing.values())
        map_data["levels"][lower]["inaccessible_floors"] = [
            f for f in map_data["levels"][lower]["inaccessible_floors"]
            if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["floors"] = [
            f for f in map_data["levels"][upper]["floors"]
            if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["inaccessible_floors"] = [
            f for f in map_data["levels"][upper]["inaccessible_floors"]
            if (f["col"], f["row"]) not in cells
        ]


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
        if not zone["kind"]:
            errors.append(f"actor_spawn_zones[{idx}] has empty `kind`")
        if zone["count"] < 0:
            errors.append(f"actor_spawn_zones[{idx}] has negative count")

    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _validate_zone_rect(zone, f"player_spawn_zones[{idx}]", map_data, errors)

    for level_idx, level in enumerate(map_data["levels"]):
        prefix = level_label(level, level_idx)
        if not level["floors"]:
            errors.append(f"{prefix}: at least one floor is required by the Rust loader")
        floor_set = {(f["col"], f["row"]) for f in level["floors"]}
        for floor in level["floors"]:
            c, r = floor["col"], floor["row"]
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: floor [{c}, {r}] is outside the grid")
        for floor in level["inaccessible_floors"]:
            c, r = floor["col"], floor["row"]
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: inaccessible floor [{c}, {r}] is outside the grid")
            if (c, r) in floor_set:
                errors.append(f"{prefix}: inaccessible floor [{c}, {r}] overlaps a floor")
        for wall in level["walls"]:
            c0, r0, c1, r1 = wall["c0"], wall["r0"], wall["c1"], wall["r1"]
            if not (grid_point_in_bounds(c0, r0, cols, rows) and grid_point_in_bounds(c1, r1, cols, rows)):
                errors.append(f"{prefix}: wall [{c0}, {r0}, {c1}, {r1}] is outside the grid-line bounds")
            if abs(c1 - c0) + abs(r1 - r0) != 1:
                errors.append(f"{prefix}: wall [{c0}, {r0}, {c1}, {r1}] is not one grid edge")

    ramp_cells_by_level: list[set[tuple[int, int]]] = [set() for _ in map_data["levels"]]
    for ramp in map_data["ramps"]:
        lower = ramp["lower_level"]
        for cell in ramp_cells(ramp):
            for level in (lower, lower + 1):
                if 0 <= level < len(ramp_cells_by_level):
                    ramp_cells_by_level[level].add(cell)
    inaccessible_by_level = [
        {(f["col"], f["row"]) for f in level["inaccessible_floors"]} for level in map_data["levels"]
    ]
    for idx, zone in enumerate(map_data["actor_spawn_zones"]):
        _check_zone_clear_of_obstructions(
            zone, f"actor_spawn_zones[{idx}]", ramp_cells_by_level, inaccessible_by_level, errors
        )
    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _check_zone_clear_of_obstructions(
            zone, f"player_spawn_zones[{idx}]", ramp_cells_by_level, inaccessible_by_level, errors
        )

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


def zone_color(kind: str) -> QColor:
    if not kind:
        return QColor(34, 197, 94)
    return tag_color(kind)


def tag_color(tag: str) -> QColor:
    digest = hashlib.md5(tag.encode("utf-8")).digest()
    hue = (digest[0] | (digest[1] << 8)) % 360
    color = QColor()
    color.setHsv(hue, 165, 220)
    return color


FACES = ("top", "bottom", "north", "south", "east", "west")

WALL_PEN_WIDTH = 6
WALL_HIGHLIGHT_WIDTH = WALL_PEN_WIDTH + 4


def face_color(seg: dict) -> QColor:
    """Color derived from the segment's full six-face material composition.
    Two segments (floor / wall / ramp) sharing the same six face values get
    the same color; differing on any face produces a different one. Saturation
    is pinned to max so hue differences read clearly; value also varies a bit
    so close hues remain distinguishable."""
    digest = hashlib.md5("|".join(seg.get(face, "") for face in FACES).encode("utf-8")).digest()
    hue = int.from_bytes(digest[:2], "big") % 360
    value = 200 + (digest[2] % 56)  # 200-255
    color = QColor()
    color.setHsv(hue, 255, value)
    return color


def expand_face_materials(obj: dict) -> dict[str, str]:
    """Expand `all` shorthand into six explicit face materials. Faces not
    explicitly set fall back to `all` (or to any other face value if `all` is
    absent). Used when reading per-segment material data from JSON."""
    fallback = obj.get("all")
    if fallback is None:
        fallback = next((obj[face] for face in FACES if face in obj), None)
    if fallback is None:
        # No materials at all on this segment — fall back to a sentinel so the
        # editor can still display something. Real maps shouldn't hit this.
        fallback = "fiberous-plaster1-ue"
    return {face: obj.get(face, fallback) for face in FACES}


def compact_face_materials(faces: dict[str, str]) -> dict:
    """Pack six face materials into the on-disk `all` + overrides shape.
    Picks the most-common face value as `all`; ties broken alphabetically for
    deterministic output."""
    counts: dict[str, int] = {}
    for face in FACES:
        if face in faces:
            counts[faces[face]] = counts.get(faces[face], 0) + 1
    if not counts:
        return {}
    best_count = max(counts.values())
    most_common = sorted(name for name, count in counts.items() if count == best_count)[0]
    if best_count <= 1:
        return {face: faces[face] for face in FACES if face in faces}
    out = {"all": most_common}
    for face in FACES:
        if face in faces and faces[face] != most_common:
            out[face] = faces[face]
    return out


def materials_summary(seg: dict) -> str:
    """One-line summary of a segment's six face materials, using the same
    `all`/overrides compaction as the on-disk shape."""
    compact = compact_face_materials(seg)
    return ", ".join(f"{k}={v}" for k, v in compact.items())


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


# ============================================================================
# Drag / paint geometry helpers (cell rects, wall edges, ramp shapes)
# ============================================================================


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


# ============================================================================
# Undo / Redo
# ============================================================================


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


# ============================================================================
# Canvas widget
# ============================================================================


class Canvas(QWidget):
    def __init__(self, window: "EditorWindow"):
        super().__init__()
        self.window = window
        self.drag_start_cell: tuple[int, int] | None = None
        self.drag_start_point: tuple[int, int] | None = None
        self.drag_current_cell: tuple[int, int] | None = None
        self.drag_current_point: tuple[int, int] | None = None
        # `hover_kind` is one of "floor", "inaccessible", "ramp", "wall", or
        # None. `hover_target` is the segment dict being hovered. Set by
        # mouseMoveEvent in material modes; consumed by paintEvent to draw an
        # outline highlight, and by `_hover_label` for the popup near the
        # cursor.
        self.hover_kind: str | None = None
        self.hover_target: dict | None = None
        self._hover_label = QLabel(self)
        self._hover_label.setStyleSheet(
            "background-color: rgba(15, 23, 42, 230);"
            "color: #f1f5f9;"
            "border: 1px solid #475569;"
            "border-radius: 4px;"
            "padding: 4px 8px;"
        )
        self._hover_label.hide()
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
        overlay = self.window.show_material_overlay
        default_floor = QColor("#454f5b")
        for floor in level["floors"]:
            col, row = floor["col"], floor["row"]
            color = face_color(floor) if overlay else default_floor
            painter.setBrush(color)
            painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))
        painter.setBrush(default_floor)
        for floor in level["inaccessible_floors"]:
            col, row = floor["col"], floor["row"]
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
                self.window.mode in (MODE_FLOOR, MODE_FLOOR_MATERIAL, *ERASE_MODES)
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
            elif self.window.mode == MODE_FLOOR_MATERIAL:
                color = QColor(236, 72, 153, 120)
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

        default_wall_color = QColor("#f1f5f9")
        for wall in level["walls"]:
            color = face_color(wall) if overlay else default_wall_color
            painter.setPen(QPen(color, WALL_PEN_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(
                wall["c0"] * cell, wall["r0"] * cell,
                wall["c1"] * cell, wall["r1"] * cell,
            )

        # Wall-material drag preview is grid-point based: a 2D rectangle when
        # the drag spans both axes, or a thick line when it stays on a single
        # row or column (so single-line selections are visible). Painted after
        # walls so it sits on top of them.
        if (
            self.window.mode == MODE_WALL_MATERIAL
            and self.drag_start_point
            and self.drag_current_point
        ):
            sc, sr = self.drag_start_point
            ec, er = self.drag_current_point
            c0_pt, c1_pt = sorted((sc, ec))
            r0_pt, r1_pt = sorted((sr, er))
            highlight = QColor(236, 72, 153, 230)
            if c0_pt == c1_pt or r0_pt == r1_pt:
                painter.setPen(QPen(highlight, WALL_HIGHLIGHT_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
                painter.drawLine(c0_pt * cell, r0_pt * cell, c1_pt * cell, r1_pt * cell)
                painter.setPen(Qt.PenStyle.NoPen)
            else:
                painter.setBrush(QColor(236, 72, 153, 110))
                painter.setPen(QPen(highlight, 2))
                painter.drawRect(QRectF(c0_pt * cell, r0_pt * cell, (c1_pt - c0_pt) * cell, (r1_pt - r0_pt) * cell))
                painter.setPen(Qt.PenStyle.NoPen)

        # Hover highlight for material modes — drawn last so it sits on top.
        self.paint_hover_highlight(painter, cell, level_idx)

    def paint_hover_highlight(self, painter: QPainter, cell: float, level_idx: int) -> None:
        if self.hover_target is None:
            return
        highlight = QColor(236, 72, 153, 230)  # magenta, opaque-ish
        if self.hover_kind in ("floor", "inaccessible"):
            col, row = self.hover_target["col"], self.hover_target["row"]
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.setPen(QPen(highlight, 3))
            painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))
        elif self.hover_kind == "ramp":
            ramp = self.hover_target
            if level_idx not in (ramp["lower_level"], ramp["lower_level"] + 1):
                return
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.setPen(QPen(highlight, 3))
            for col, row in ramp_cells(ramp):
                painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))
        elif self.hover_kind == "wall":
            wall = self.hover_target
            painter.setPen(QPen(highlight, WALL_HIGHLIGHT_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(
                wall["c0"] * cell, wall["r0"] * cell,
                wall["c1"] * cell, wall["r1"] * cell,
            )
        painter.setPen(Qt.PenStyle.NoPen)

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
        outline_color = zone_color(zone["kind"])
        fill_color = QColor(outline_color)
        fill_color.setAlpha(70)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        label = f"{zone['kind']}:{zone['count']}" if zone["kind"] else "(empty)"
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
        if self.window.show_material_overlay:
            painter.setBrush(face_color(ramp))
        else:
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
            if self.window.mode in (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL):
                self._update_material_hover(event.position())
            return
        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.window.update_spawn_zone_edit_drag(event.position(), self.cell_size())
            self.update()
            return
        self.drag_current_cell = self.point_to_cell(event.position()) or self.drag_current_cell
        self.drag_current_point = self.point_to_grid_point(event.position())
        self.update()

    def leaveEvent(self, _event) -> None:
        self._clear_hover()

    def _clear_hover(self) -> None:
        if self.hover_target is not None:
            self.hover_kind = None
            self.hover_target = None
            self.update()
        self._hover_label.hide()

    def _update_material_hover(self, pos) -> None:
        cell_size = self.cell_size()
        level_idx = self.window.current_level
        level = self.window.map_data["levels"][level_idx]

        kind: str | None = None
        target: dict | None = None
        tooltip: str | None = None

        if self.window.mode == MODE_FLOOR_MATERIAL:
            cell = self.point_to_cell(pos)
            if cell is not None:
                col, row = cell
                # Prefer ramp when one covers the hovered cell, since the
                # ramp visually replaces the floor underneath.
                ramp_hit: dict | None = None
                for ramp in self.window.map_data["ramps"]:
                    if ramp["lower_level"] == level_idx and (col, row) in ramp_cells(ramp):
                        ramp_hit = ramp
                        break
                if ramp_hit is not None:
                    kind, target = "ramp", ramp_hit
                    tooltip = f"Ramp\n{materials_summary(ramp_hit)}"
                else:
                    for f in level["floors"]:
                        if f["col"] == col and f["row"] == row:
                            kind, target = "floor", f
                            tooltip = f"Floor\n{materials_summary(f)}"
                            break
                    else:
                        for f in level["inaccessible_floors"]:
                            if f["col"] == col and f["row"] == row:
                                kind, target = "inaccessible", f
                                tooltip = f"Inaccessible floor\n{materials_summary(f)}"
                                break
        else:  # MODE_WALL_MATERIAL
            px = pos.x() / cell_size
            py = pos.y() / cell_size
            for wall in level["walls"]:
                wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
                if point_near_wall(px, py, wall_arr, tolerance=0.2):
                    kind, target = "wall", wall
                    tooltip = f"Wall\n{materials_summary(wall)}"
                    break

        changed = (kind, id(target)) != (self.hover_kind, id(self.hover_target))
        self.hover_kind = kind
        self.hover_target = target
        if changed:
            self.update()
        if tooltip is not None:
            self._hover_label.setText(tooltip)
            self._hover_label.adjustSize()
            # Offset slightly so the popup doesn't sit directly under the
            # cursor; clamp inside the canvas so it never gets clipped.
            x = int(pos.x()) + 16
            y = int(pos.y()) + 16
            x = max(0, min(x, self.width() - self._hover_label.width() - 4))
            y = max(0, min(y, self.height() - self._hover_label.height() - 4))
            self._hover_label.move(x, y)
            self._hover_label.show()
            self._hover_label.raise_()
        else:
            self._hover_label.hide()

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
        elif self.window.mode == MODE_FLOOR_MATERIAL and self.drag_start_cell and self.drag_current_cell:
            self.window.assign_floor_materials_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_WALL_MATERIAL and self.drag_start_point and self.drag_current_point:
            self.window.assign_wall_materials_rect(self.drag_start_point, self.drag_current_point)
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
                list_name = picked.list_name
                if list_name == ACTOR_ZONE_LIST:
                    menu.addAction("Edit Fields...", lambda: self.window.edit_selected_spawn_zone_fields())
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


# ============================================================================
# Editor window
# ============================================================================


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
        # No default kind: the dialog opens with Kind blank on the first paint
        # of a session and remembers the last value across subsequent paints.
        self.recent_actor_spawn_kind: str = ""
        self.recent_actor_spawn_count: int = DEFAULT_ACTOR_COUNT
        self.selected_spawn_zone_ref: ZoneRef | None = None
        self.spawn_zone_drag: SpawnZoneDrag | None = None
        self.show_material_overlay = False
        # Material used as the default for newly painted floors / walls / ramps.
        # The user picks a different one from the materials palette.
        self.current_material: str = "fiberous-plaster1-ue"
        self.materials_catalog: list[str] = load_materials_catalog(self.path)

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

        map_menu = self.menuBar().addMenu("&Map")
        self.add_menu_action(map_menu, "&Resize Map...", None, self.resize_map)
        map_menu.addSeparator()
        self.material_overlay_action = QAction("Show &Material Overlay", self)
        self.material_overlay_action.setCheckable(True)
        self.material_overlay_action.setShortcut(QKeySequence("M"))
        self.material_overlay_action.toggled.connect(self.set_material_overlay)
        map_menu.addAction(self.material_overlay_action)

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
            ref = self.selected_spawn_zone_ref
            if 0 <= ref.index < len(self.map_data[ref.list_name]):
                prior_selection = (ref.list_name, copy.deepcopy(self.map_data[ref.list_name][ref.index]))
        self.map_data = canonicalize_map(map_data)
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        if prior_selection is not None:
            list_name, snapshot = prior_selection
            self.selected_spawn_zone_ref = self._zone_ref_after_change(list_name, snapshot)
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

    def set_material_overlay(self, enabled: bool) -> None:
        self.show_material_overlay = enabled
        self.canvas.update()

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

    def _face_materials_for_current(self) -> dict[str, str]:
        return {face: self.current_material for face in FACES}

    def _new_floor(self, col: int, row: int) -> dict:
        return {"col": col, "row": row, **self._face_materials_for_current()}

    def _new_wall(self, c0: int, r0: int, c1: int, r1: int) -> dict:
        return {"c0": c0, "r0": r0, "c1": c1, "r1": r1, **self._face_materials_for_current()}

    def _new_ramp(self, low: list[int], high: list[int], lower_level: int) -> dict:
        return {
            "low": low,
            "high": high,
            "lower_level": lower_level,
            **self._face_materials_for_current(),
        }

    def add_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        existing_floors = {(f["col"], f["row"]): f for f in level["floors"]}
        existing_inacc = {(f["col"], f["row"]): f for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                if (col, row) not in existing_floors:
                    existing_floors[(col, row)] = self._new_floor(col, row)
                existing_inacc.pop((col, row), None)
        level["floors"] = list(existing_floors.values())
        level["inaccessible_floors"] = list(existing_inacc.values())
        self.apply_change("Paint Floor", after)

    def add_inaccessible_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        existing_floors = {(f["col"], f["row"]): f for f in level["floors"]}
        existing_inacc = {(f["col"], f["row"]): f for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                existing_floors.pop((col, row), None)
                if (col, row) not in existing_inacc:
                    existing_inacc[(col, row)] = self._new_floor(col, row)
        level["floors"] = list(existing_floors.values())
        level["inaccessible_floors"] = list(existing_inacc.values())
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
        result = self.prompt_for_actor_spawn_fields()
        if result is None:
            return
        kind, count = result
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": count,
        }
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Actor Spawn Zone", after)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.selected_spawn_zone_ref = self._zone_ref_after_change(ACTOR_ZONE_LIST, new_zone)

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
        self.selected_spawn_zone_ref = self._zone_ref_after_change(PLAYER_ZONE_LIST, new_zone)

    def prompt_for_actor_spawn_fields(self, kind: str | None = None, count: int | None = None) -> tuple[str, int] | None:
        return ActorSpawnFieldsDialog.prompt(
            self,
            kind if kind is not None else self.recent_actor_spawn_kind,
            count if count is not None else self.recent_actor_spawn_count,
        )

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        edges = wall_segments_between(start, end)
        if not edges:
            return
        after = copy.deepcopy(self.map_data)
        existing = {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]])): w
            for w in after["levels"][self.current_level]["walls"]
        }
        for edge in edges:
            key = tuple(normalized_wall(edge))
            if key not in existing:
                c0, r0, c1, r1 = key
                existing[key] = self._new_wall(c0, r0, c1, r1)
        after["levels"][self.current_level]["walls"] = list(existing.values())
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
        new_ramp = self._new_ramp(low, high, lower_level)
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

    def assign_floor_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]

        def floor_in_rect(f: dict) -> bool:
            return c0 <= f["col"] < c1 and r0 <= f["row"] < r1

        def ramp_in_rect(ramp: dict) -> bool:
            return level_idx == ramp["lower_level"] and rects_overlap(
                (c0, r0, c1, r1), ramp_rect(ramp)
            )

        affected_floors = [f for f in level["floors"] if floor_in_rect(f)] + [
            f for f in level["inaccessible_floors"] if floor_in_rect(f)
        ]
        affected_ramps = [r for r in self.map_data["ramps"] if ramp_in_rect(r)]
        if not affected_floors and not affected_ramps:
            self.statusBar().showMessage("No floor segments in selection.", 4000)
            return
        seed = (affected_floors or affected_ramps)[0]
        result = MaterialAssignmentDialog.prompt(
            self, "Floor Materials",
            f"{len(affected_floors)} floor cell(s), {len(affected_ramps)} ramp(s) in selection",
            self.materials_catalog,
            {face: seed.get(face, self.current_material) for face in FACES},
        )
        if result is None:
            return
        after = copy.deepcopy(self.map_data)
        for floor in after["levels"][level_idx]["floors"]:
            if floor_in_rect(floor):
                floor.update(result)
        for floor in after["levels"][level_idx]["inaccessible_floors"]:
            if floor_in_rect(floor):
                floor.update(result)
        # Update top/bottom faces only on overlapping ramps; their N/S/E/W
        # faces are owned by the wall-material edit mode.
        for ramp in after["ramps"]:
            if ramp_in_rect(ramp):
                ramp["top"] = result["top"]
                ramp["bottom"] = result["bottom"]
        self.apply_change("Assign Floor Materials", after)

    def assign_wall_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        # Selection is a 2D rectangle defined by two grid points. A wall is
        # "in" the selection iff both endpoints lie inside the rect (so walls
        # only touching at a corner are not affected). A flat selection
        # (start and end share a row or column) collapses to a single grid
        # line — exactly the walls along that row/column.
        c0, c1 = sorted([start[0], end[0]])
        r0, r1 = sorted([start[1], end[1]])
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]

        def edge_inside(wall: dict) -> bool:
            return (
                c0 <= wall["c0"] <= c1
                and c0 <= wall["c1"] <= c1
                and r0 <= wall["r0"] <= r1
                and r0 <= wall["r1"] <= r1
            )

        # Ramps live in cells — only count the ones whose footprint intersects
        # the *cell* rect implied by this grid-point selection. A flat
        # (single-line) wall selection has an empty cell rect and so doesn't
        # touch any ramps.
        cell_rect = (c0, r0, c1, r1)

        def ramp_in_rect(ramp: dict) -> bool:
            return level_idx == ramp["lower_level"] and rects_overlap(
                cell_rect, ramp_rect(ramp)
            )

        affected_walls = [w for w in level["walls"] if edge_inside(w)]
        affected_ramps = [r for r in self.map_data["ramps"] if ramp_in_rect(r)]
        if not affected_walls and not affected_ramps:
            self.statusBar().showMessage("No wall edges in selection.", 4000)
            return
        seed = (affected_walls or affected_ramps)[0]
        result = MaterialAssignmentDialog.prompt(
            self, "Wall Materials",
            f"{len(affected_walls)} wall edge(s), {len(affected_ramps)} ramp(s) in selection",
            self.materials_catalog,
            {face: seed.get(face, self.current_material) for face in FACES},
        )
        if result is None:
            return
        after = copy.deepcopy(self.map_data)
        for wall in after["levels"][level_idx]["walls"]:
            if edge_inside(wall):
                wall.update(result)
        # Update N/S/E/W faces only on overlapping ramps; their top/bottom
        # faces are owned by the floor-material edit mode.
        for ramp in after["ramps"]:
            if ramp_in_rect(ramp):
                for face in ("north", "south", "east", "west"):
                    ramp[face] = result[face]
        self.apply_change("Assign Wall Materials", after)

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
                if not (c0 <= floor["col"] < c1 and r0 <= floor["row"] < r1)
            ]
            level["inaccessible_floors"] = [
                floor
                for floor in level["inaccessible_floors"]
                if not (c0 <= floor["col"] < c1 and r0 <= floor["row"] < r1)
            ]
        level["walls"] = [
            wall
            for wall in level["walls"]
            if not wall_overlaps_rect([wall["c0"], wall["r0"], wall["c1"], wall["r1"]], (c0, r0, c1, r1))
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
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr):
                return ("Wall", tuple(wall_arr))
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
        if any(f["col"] == col and f["row"] == row for f in level["floors"]):
            return ("Floor", (col, row))
        if any(f["col"] == col and f["row"] == row for f in level["inaccessible_floors"]):
            return ("Inaccessible Floor", (col, row))
        return None

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        kind, value = hit
        if preserve_floors and kind in FLOOR_HIT_KINDS:
            return
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if kind == "Floor":
            level["floors"] = [
                floor for floor in level["floors"] if (floor["col"], floor["row"]) != value
            ]
        elif kind == "Inaccessible Floor":
            level["inaccessible_floors"] = [
                floor for floor in level["inaccessible_floors"]
                if (floor["col"], floor["row"]) != value
            ]
        elif kind == "Spawn Zone":
            list_name, target_idx = value
            if 0 <= target_idx < len(after[list_name]):
                del after[list_name][target_idx]
                if self.selected_spawn_zone_ref == ZoneRef(list_name, target_idx):
                    self.selected_spawn_zone_ref = None
        elif kind == "Wall":
            level["walls"] = [
                wall for wall in level["walls"]
                if tuple(normalized_wall([wall["c0"], wall["r0"], wall["c1"], wall["r1"]])) != value
            ]
        elif kind == "Ramp":
            lower, low, high = value
            after["ramps"] = [
                ramp
                for ramp in after["ramps"]
                if (ramp["lower_level"], tuple(ramp["low"]), tuple(ramp["high"])) != (lower, low, high)
            ]
        self.apply_change(f"Erase {kind}", after)

    def resize_map(self) -> None:
        result = ResizeMapDialog.prompt(
            self, self.map_data["grid_cols"], self.map_data["grid_rows"]
        )
        if result is None:
            return
        new_cols, new_rows, anchor_x, anchor_y = result
        if new_cols == self.map_data["grid_cols"] and new_rows == self.map_data["grid_rows"]:
            return
        after = resize_map_data(self.map_data, new_cols, new_rows, anchor_x, anchor_y)

        before_floors = sum(len(l["floors"]) for l in self.map_data["levels"])
        before_inacc = sum(len(l["inaccessible_floors"]) for l in self.map_data["levels"])
        before_walls = sum(len(l["walls"]) for l in self.map_data["levels"])
        before_zones = len(self.map_data["actor_spawn_zones"]) + len(self.map_data["player_spawn_zones"])
        before_ramps = len(self.map_data["ramps"])
        after_floors = sum(len(l["floors"]) for l in after["levels"])
        after_inacc = sum(len(l["inaccessible_floors"]) for l in after["levels"])
        after_walls = sum(len(l["walls"]) for l in after["levels"])
        after_zones = len(after["actor_spawn_zones"]) + len(after["player_spawn_zones"])
        after_ramps = len(after["ramps"])
        dropped_floors = before_floors - after_floors
        dropped_inacc = before_inacc - after_inacc
        dropped_walls = before_walls - after_walls
        dropped_zones = before_zones - after_zones
        dropped_ramps = before_ramps - after_ramps
        if any(n > 0 for n in (dropped_floors, dropped_inacc, dropped_walls, dropped_zones, dropped_ramps)):
            parts = []
            if dropped_floors:
                parts.append(f"{dropped_floors} floor cell(s)")
            if dropped_inacc:
                parts.append(f"{dropped_inacc} inaccessible-floor cell(s)")
            if dropped_walls:
                parts.append(f"{dropped_walls} wall(s)")
            if dropped_zones:
                parts.append(f"{dropped_zones} spawn zone(s)")
            if dropped_ramps:
                parts.append(f"{dropped_ramps} ramp(s)")
            response = QMessageBox.question(
                self,
                "Resize Map",
                "Resizing will drop:\n  - " + "\n  - ".join(parts) + "\n\nContinue?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.Cancel,
            )
            if response != QMessageBox.StandardButton.Yes:
                return

        self.apply_change("Resize Map", after)
        self.resize_to_map()

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
            "Actor Spawn Zone (Paint): drag a rectangle, then enter Kind and Count.\n"
            "Player Spawn Zone (Paint): drag a rectangle. No prompt — players spawn anywhere in any player zone.\n"
            "Spawn Zone (Edit): click a zone to select; drag the body to move, drag a corner/edge handle to resize. Right-click to edit fields (actor zones only) or delete.\n"
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
        if not (0 <= ref.index < len(self.map_data[ref.list_name])):
            return None
        return self.map_data[ref.list_name][ref.index]

    def set_selected_spawn_zone(self, ref: ZoneRef | None) -> None:
        if ref is None:
            self.selected_spawn_zone_ref = None
        elif ref.list_name in SPAWN_ZONE_LISTS and 0 <= ref.index < len(self.map_data[ref.list_name]):
            self.selected_spawn_zone_ref = ref
        else:
            self.selected_spawn_zone_ref = None
        self.canvas.update()

    def _find_zone_index(self, list_name: str, target: dict) -> int | None:
        key = zone_key(list_name, target)
        for idx, zone in enumerate(self.map_data[list_name]):
            if zone_key(list_name, zone) == key:
                return idx
        return None

    def _zone_ref_after_change(self, list_name: str, target: dict) -> ZoneRef | None:
        new_idx = self._find_zone_index(list_name, target)
        return ZoneRef(list_name, new_idx) if new_idx is not None else None

    def spawn_zone_at(self, pos, cell_size: float) -> ZoneRef | None:
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        # Iterate in reverse so the most-recently-painted wins. Actor zones
        # take priority over player zones when both cover the cell — author
        # is more likely to want to edit the actor entries.
        for list_name in (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST):
            for idx in range(len(self.map_data[list_name]) - 1, -1, -1):
                zone = self.map_data[list_name][idx]
                if zone["level"] == self.current_level and zone_contains_cell(zone, col, row):
                    return ZoneRef(list_name, idx)
        return None

    def begin_spawn_zone_edit_press(self, pos, cell_size: float) -> None:
        # Try a handle on the currently selected zone first.
        zone = self.selected_spawn_zone()
        if zone is not None and zone["level"] == self.current_level:
            handle = self._handle_at_pos(zone, pos, cell_size)
            if handle is not None:
                assert self.selected_spawn_zone_ref is not None
                ref = self.selected_spawn_zone_ref
                self.spawn_zone_drag = SpawnZoneDrag(
                    list_name=ref.list_name,
                    index=ref.index,
                    handle=handle,
                    origin=(pos.x() / cell_size, pos.y() / cell_size),
                    original_zone=copy.deepcopy(zone),
                )
                return
        # Otherwise pick the zone under the cursor.
        ref = self.spawn_zone_at(pos, cell_size)
        if ref is None:
            self.set_selected_spawn_zone(None)
            self.spawn_zone_drag = None
            return
        self.set_selected_spawn_zone(ref)
        self.spawn_zone_drag = SpawnZoneDrag(
            list_name=ref.list_name,
            index=ref.index,
            handle="move",
            origin=(pos.x() / cell_size, pos.y() / cell_size),
            original_zone=copy.deepcopy(self.map_data[ref.list_name][ref.index]),
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
        drag = self.spawn_zone_drag
        if drag is None or not hasattr(self, "_drag_current_cell_pos"):
            return None
        ox, oy = drag.origin
        cx, cy = self._drag_current_cell_pos
        dx_cells = round(cx - ox)
        dy_cells = round(cy - oy)
        c0, r0, c1, r1 = zone_rect(drag.original_zone)
        cols_max = self.map_data["grid_cols"]
        rows_max = self.map_data["grid_rows"]
        if drag.handle == "move":
            new_c0 = max(0, min(cols_max - (c1 - c0), c0 + dx_cells))
            new_r0 = max(0, min(rows_max - (r1 - r0), r0 + dy_cells))
            return (new_c0, new_r0, new_c0 + (c1 - c0), new_r0 + (r1 - r0))
        new_c0, new_r0, new_c1, new_r1 = c0, r0, c1, r1
        if "n" in drag.handle:
            new_r0 = max(0, min(r1 - 1, r0 + dy_cells))
        if "s" in drag.handle:
            new_r1 = max(r0 + 1, min(rows_max, r1 + dy_cells))
        if "w" in drag.handle:
            new_c0 = max(0, min(c1 - 1, c0 + dx_cells))
        if "e" in drag.handle:
            new_c1 = max(c0 + 1, min(cols_max, c1 + dx_cells))
        return (new_c0, new_r0, new_c1, new_r1)

    def commit_spawn_zone_edit_drag(self) -> None:
        drag = self.spawn_zone_drag
        if drag is None:
            return
        candidate = self.spawn_zone_candidate_rect()
        self.spawn_zone_drag = None
        if hasattr(self, "_drag_current_cell_pos"):
            del self._drag_current_cell_pos
        if candidate is None:
            return
        c0, r0, c1, r1 = candidate
        if (c0, r0, c1, r1) == zone_rect(drag.original_zone):
            return
        if not (0 <= drag.index < len(self.map_data[drag.list_name])):
            return
        after = copy.deepcopy(self.map_data)
        zone = after[drag.list_name][drag.index]
        zone["cols"] = [c0, c1]
        zone["rows"] = [r0, r1]
        self.apply_change("Edit Spawn Zone", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(drag.list_name, zone)

    def edit_selected_spawn_zone_fields(self) -> None:
        zone = self.selected_spawn_zone()
        ref = self.selected_spawn_zone_ref
        if zone is None or ref is None:
            return
        if ref.list_name != ACTOR_ZONE_LIST:
            return  # Player zones have no editable fields.
        result = self.prompt_for_actor_spawn_fields(zone["kind"], zone["count"])
        if result is None:
            return
        kind, count = result
        after = copy.deepcopy(self.map_data)
        if not (0 <= ref.index < len(after[ref.list_name])):
            return
        after[ref.list_name][ref.index]["kind"] = kind
        after[ref.list_name][ref.index]["count"] = count
        self.apply_change("Edit Actor Spawn Zone", after)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.selected_spawn_zone_ref = self._zone_ref_after_change(ref.list_name, after[ref.list_name][ref.index])

    def delete_selected_spawn_zone(self) -> None:
        ref = self.selected_spawn_zone_ref
        if ref is None:
            return
        if not (0 <= ref.index < len(self.map_data[ref.list_name])):
            return
        after = copy.deepcopy(self.map_data)
        del after[ref.list_name][ref.index]
        self.selected_spawn_zone_ref = None
        self.apply_change("Delete Spawn Zone", after)

    def closeEvent(self, event) -> None:
        if self.confirm_discard_changes():
            event.accept()
        else:
            event.ignore()


# ============================================================================
# Entry point
# ============================================================================


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

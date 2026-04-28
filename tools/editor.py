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

from PySide6.QtCore import QPoint, QRectF, QSize, Qt, QTimer
from PySide6.QtGui import QAction, QColor, QKeySequence, QPainter, QPen, QShortcut, QUndoCommand, QUndoStack
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
DEFAULT_MAP = REPO_ROOT / "server" / "assets" / "maps" / "default.json"
SUPPORTED_VERSION = 1

MODE_FLOOR = "Floor"
MODE_PLAYER_SPAWN = "Player Spawn"
MODE_WALL = "Wall"
MODE_RAMP_UP = "Ramp (Up)"
MODE_RAMP_DOWN = "Ramp (Down)"
MODE_ERASE = "Erase"
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
MODES = [MODE_FLOOR, MODE_PLAYER_SPAWN, MODE_WALL, MODE_RAMP_UP, MODE_RAMP_DOWN, MODE_ERASE]

MIN_CELL = 12.0
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20


def empty_map() -> dict:
    return {
        "grid_cols": DEFAULT_GRID_COLS,
        "grid_rows": DEFAULT_GRID_ROWS,
        "player_spawn_fields": [[0, 0, 0], [0, 1, 0], [0, 0, 1], [0, 1, 1]],
        "levels": [{"name": "Level 0", "floors": [], "walls": []}],
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


def format_map_file(wrapper: dict) -> str:
    map_data = wrapper["map"]
    lines = [
        "{",
        f'  "version": {wrapper["version"]},',
        '  "map": {',
        f'    "grid_cols": {map_data["grid_cols"]},',
        f'    "grid_rows": {map_data["grid_rows"]},',
        *with_trailing_comma(format_point_array("player_spawn_fields", map_data["player_spawn_fields"], 4)),
        '    "levels": [',
    ]

    for level_idx, level in enumerate(map_data["levels"]):
        lines.extend(
            [
                "      {",
                f'        "name": {json_scalar(level["name"])},',
                *with_trailing_comma(format_point_array("floors", level["floors"], 8)),
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
    player_spawn_fields = []
    for field in map_data.get("player_spawn_fields", []):
        if len(field) == 2:
            c, r = field
            player_spawn_fields.append([0, int(c), int(r)])
        elif len(field) == 3:
            level, c, r = field
            player_spawn_fields.append([int(level), int(c), int(r)])
        else:
            player_spawn_fields.append([0, -1, -1])
    levels = []
    for idx, level in enumerate(map_data.get("levels", [])):
        levels.append(
            {
                "name": str(level.get("name") or f"Level {idx}"),
                "floors": [[int(c), int(r)] for c, r in level.get("floors", [])],
                "walls": [[int(c0), int(r0), int(c1), int(r1)] for c0, r0, c1, r1 in level.get("walls", [])],
            }
        )
    if not levels:
        levels = [{"name": "Level 0", "floors": [], "walls": []}]

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
        "player_spawn_fields": player_spawn_fields,
        "levels": levels,
        "ramps": ramps,
    }


def canonicalize_map(map_data: dict) -> dict:
    b = normalize_map(copy.deepcopy(map_data))
    enforce_ramp_floor_rules(b)
    b["player_spawn_fields"] = sorted(
        {(level, c, r) for level, c, r in b["player_spawn_fields"]},
        key=lambda p: (p[0], p[2], p[1]),
    )
    b["player_spawn_fields"] = [[level, c, r] for level, c, r in b["player_spawn_fields"]]
    for level in b["levels"]:
        level["floors"] = sorted({(c, r) for c, r in level["floors"]}, key=lambda p: (p[1], p[0]))
        level["floors"] = [[c, r] for c, r in level["floors"]]

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
        lower_floors.update(cells)
        upper_floors.difference_update(cells)
        map_data["levels"][lower]["floors"] = [[c, r] for c, r in lower_floors]
        map_data["levels"][upper]["floors"] = [[c, r] for c, r in upper_floors]


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
    if not map_data["player_spawn_fields"]:
        errors.append("at least one player spawn field is required by the Rust loader")

    spawn_fields = set()
    for field in map_data["player_spawn_fields"]:
        level, c, r = field
        if not (0 <= level < len(map_data["levels"])):
            errors.append(f"player spawn field {field} has an invalid level")
        if not (0 <= c < cols and 0 <= r < rows):
            errors.append(f"player spawn field {field} is outside the grid")
        if tuple(field) in spawn_fields:
            errors.append(f"duplicate player spawn field {field}")
        spawn_fields.add(tuple(field))

    for level_idx, level in enumerate(map_data["levels"]):
        prefix = level_label(level, level_idx)
        if not level["floors"]:
            errors.append(f"{prefix}: at least one floor is required by the Rust loader")
        for floor in level["floors"]:
            c, r = floor
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: floor {floor} is outside the grid")
        for wall in level["walls"]:
            c0, r0, c1, r1 = wall
            if not (grid_point_in_bounds(c0, r0, cols, rows) and grid_point_in_bounds(c1, r1, cols, rows)):
                errors.append(f"{prefix}: wall {wall} is outside the grid-line bounds")
            if abs(c1 - c0) + abs(r1 - r0) != 1:
                errors.append(f"{prefix}: wall {wall} is not one grid edge")

    for field in spawn_fields:
        level, c, r = field
        if not (0 <= level < len(map_data["levels"])):
            continue
        floors = {tuple(floor) for floor in map_data["levels"][level]["floors"]}
        if (c, r) not in floors:
            errors.append(f"player spawn field {list(field)} is not a floor on level {level}")

    for ramp in map_data["ramps"]:
        msg = ramp_error(ramp["low"], ramp["high"], ramp["lower_level"], cols, rows, len(map_data["levels"]))
        if msg:
            errors.append(f"ramp {ramp}: {msg}")
        lower = ramp["lower_level"]
        for col, row in ramp_cells(ramp):
            for level in (lower, lower + 1):
                field = (level, col, row)
                if field in spawn_fields:
                    errors.append(f"player spawn field {list(field)} overlaps a ramp on level {level}")
    return errors


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

        for ramp in self.window.map_data["ramps"]:
            lower = ramp["lower_level"]
            if level_idx not in (lower, lower + 1):
                continue
            self.paint_ramp(painter, ramp, cell, lower == level_idx)

        self.paint_player_spawn_fields(painter, cell, level_idx)

        if (
            self.drag_start_cell
            and self.drag_current_cell
            and (
                self.window.mode in (MODE_FLOOR, MODE_ERASE)
                or self.window.mode == MODE_PLAYER_SPAWN
            )
        ):
            c0, r0, c1, r1 = rect_from_cells(self.drag_start_cell, self.drag_current_cell)
            if self.window.mode == MODE_ERASE:
                color = QColor(248, 113, 113, 120)
            elif self.window.mode == MODE_FLOOR:
                color = QColor(111, 180, 255, 120)
            else:
                color = QColor(34, 197, 94, 120)
            painter.setBrush(color)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))

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

    def paint_player_spawn_fields(self, painter: QPainter, cell: float, level_idx: int) -> None:
        painter.setPen(QPen(QColor("#86efac"), 2))
        painter.setBrush(QColor(34, 197, 94, 95))
        for level, col, row in self.window.map_data["player_spawn_fields"]:
            if level != level_idx:
                continue
            rect = QRectF(col * cell + 4, row * cell + 4, cell - 8, cell - 8)
            painter.drawRect(rect)
            painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, "S")

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
        self.drag_start_cell = self.point_to_cell(event.position())
        self.drag_current_cell = self.drag_start_cell
        self.drag_start_point = self.point_to_grid_point(event.position())
        self.drag_current_point = self.drag_start_point
        self.update()

    def mouseMoveEvent(self, event) -> None:
        if not (event.buttons() & Qt.MouseButton.LeftButton):
            return
        self.drag_current_cell = self.point_to_cell(event.position()) or self.drag_current_cell
        self.drag_current_point = self.point_to_grid_point(event.position())
        self.update()

    def mouseReleaseEvent(self, event) -> None:
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if self.window.mode == MODE_FLOOR and self.drag_start_cell and self.drag_current_cell:
            self.window.add_floor_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_PLAYER_SPAWN and self.drag_start_cell and self.drag_current_cell:
            self.window.add_player_spawn_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_WALL and self.drag_start_point and self.drag_current_point:
            self.window.add_wall_line(self.drag_start_point, snapped_wall_end(self.drag_start_point, self.drag_current_point))
        elif self.window.mode in RAMP_MODES and self.drag_start_cell and self.drag_current_cell:
            self.window.add_ramp(self.drag_start_cell, self.drag_current_cell, self.window.mode)
        elif self.window.mode == MODE_ERASE:
            if self.drag_start_cell and self.drag_current_cell and self.drag_start_cell != self.drag_current_cell:
                self.window.erase_cell_rect(self.drag_start_cell, self.drag_current_cell)
            else:
                self.window.erase_at(event.position(), self.cell_size())
        self.clear_drag()
        self.update()

    def contextMenuEvent(self, event) -> None:
        hit = self.window.hit_at(event.pos(), self.cell_size())
        menu = QMenu(self)
        if hit:
            menu.addAction(f"Erase {hit[0]}", lambda: self.window.erase_hit(hit))
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

        self.canvas = Canvas(self)
        self.setCentralWidget(self.canvas)
        self.setWindowTitle("Cuboid Wars Editor")
        self.resize(920, 760)

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
        self.map_data = canonicalize_map(map_data)
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
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
        floors = {tuple(f) for f in after["levels"][self.current_level]["floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                floors.add((col, row))
        after["levels"][self.current_level]["floors"] = [[c, r] for c, r in floors]
        self.apply_change("Paint Floor", after)

    def add_player_spawn_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        fields = {tuple(f) for f in after["player_spawn_fields"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                fields.add((self.current_level, col, row))
        after["player_spawn_fields"] = [[level, c, r] for level, c, r in fields]
        self.apply_change("Paint Player Spawn", after)

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

    def erase_at(self, pos, cell_size: float) -> None:
        hit = self.hit_at(pos, cell_size)
        if hit:
            self.erase_hit(hit)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        level["floors"] = [
            floor
            for floor in level["floors"]
            if not (c0 <= floor[0] < c1 and r0 <= floor[1] < r1)
        ]
        level["walls"] = [
            wall
            for wall in level["walls"]
            if not wall_overlaps_rect(wall, (c0, r0, c1, r1))
        ]
        after["player_spawn_fields"] = [
            field
            for field in after["player_spawn_fields"]
            if not (field[0] == self.current_level and c0 <= field[1] < c1 and r0 <= field[2] < r1)
        ]
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if self.current_level not in (ramp["lower_level"], ramp["lower_level"] + 1)
            or not rects_overlap((c0, r0, c1, r1), ramp_rect(ramp))
        ]
        self.apply_change("Erase Area", after)

    def hit_at(self, pos, cell_size: float):
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        level = self.map_data["levels"][self.current_level]
        px = pos.x() / cell_size
        py = pos.y() / cell_size

        for wall in level["walls"]:
            if point_near_wall(px, py, wall):
                return ("Wall", tuple(wall))
        if [self.current_level, col, row] in self.map_data["player_spawn_fields"]:
            return ("Player Spawn", (col, row))
        for ramp in self.map_data["ramps"]:
            lower = ramp["lower_level"]
            if self.current_level not in (lower, lower + 1):
                continue
            c0, r0, c1, r1 = ramp_rect(ramp)
            if c0 <= col < c1 and r0 <= row < r1:
                return ("Ramp", (lower, tuple(ramp["low"]), tuple(ramp["high"])))
        if [col, row] in level["floors"]:
            return ("Floor", (col, row))
        return None

    def erase_hit(self, hit) -> None:
        kind, value = hit
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if kind == "Floor":
            level["floors"] = [floor for floor in level["floors"] if tuple(floor) != value]
        elif kind == "Player Spawn":
            after["player_spawn_fields"] = [
                field
                for field in after["player_spawn_fields"]
                if tuple(field) != (self.current_level, value[0], value[1])
            ]
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
        after["levels"].insert(insert_at, {"name": f"Level {insert_at}", "floors": [], "walls": []})
        for field in after["player_spawn_fields"]:
            if field[0] >= insert_at:
                field[0] += 1
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
        adjusted_fields = []
        for field in after["player_spawn_fields"]:
            level, col, row = field
            if level == removed:
                continue
            if level > removed:
                level -= 1
            adjusted_fields.append([level, col, row])
        after["player_spawn_fields"] = adjusted_fields
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
            "Player Spawn: drag cells on the selected level to add spawn fields.\n"
            "Wall: drag along grid lines to place atomic wall edges.\n"
            "Ramp (Up): drag from this level toward the upper level.\n"
            "Ramp (Down): drag from this level toward the lower level.\n"
            "Erase: click an item, drag cells to erase an area, or right-click for the context menu.",
        )

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

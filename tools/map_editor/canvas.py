"""Map editing canvas widget."""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import QPointF, QRectF, QSize, Qt
from PySide6.QtWidgets import QLabel, QMenu, QSizePolicy, QWidget

from .constants import (
    EDITOR_CELL,
    ERASE_MODES,
    FLOOR_HIT_KINDS,
    MATERIAL_MODES,
    MODE_ACTOR_SPAWN_ZONE,
    MODE_BARRIER,
    MODE_EQUIPMENT_ERASER,
    MODE_BRIDGE_PLATE,
    MODE_ERASE_KEEP_FLOORS,
    MODE_FIREWORK_PLATE,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_SELECT,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    MODE_PLAYER_SPAWN_ZONE,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_MATERIAL,
    MODE_WALL,
    MODE_WALL_MATERIAL,
    RAMP_MODES,
)
from .display import (
    materials_summary,
)
from .types import ZoneRef
from .normalization import pressure_plate_key
from .geometry import (
    cell_side_from_click,
    point_near_wall,
    ramp_cells,
    snapped_wall_end,
)

from .canvas_painting import CanvasPaintingMixin
from .erasing import ERASE_GROUPS
from .viewport import Viewport
from .transforms import record_rect
from .notice import CanvasNotice

if TYPE_CHECKING:
    from .window import EditorWindow


# ============================================================================
# Release tools
# ============================================================================
# One handler per mode, dispatched from `Canvas.mouseReleaseEvent`. Adding a
# mode means adding a table entry, not another ladder rung.


def _cell_rect_tool(method: str):
    """Tool for a completed cell-rect drag: `window.<method>(start, end)`."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_cell and canvas.drag_current_cell:
            getattr(canvas.window, method)(canvas.drag_start_cell, canvas.drag_current_cell)

    return handler


def _erase_group_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.erase_group_rect(canvas.window.mode, canvas.drag_start_cell, canvas.drag_current_cell)


def _wall_line_tool(method: str):
    """Tool for a grid-point drag along a wall line; the end snaps axis-aligned."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_point and canvas.drag_current_point:
            getattr(canvas.window, method)(
                canvas.drag_start_point,
                snapped_wall_end(canvas.drag_start_point, canvas.drag_current_point),
            )

    return handler


def _click_place_tool(add_method: str):
    """Click places on the released cell: `window.<add_method>(col, row)`."""

    def handler(canvas: "Canvas", event) -> None:
        cell = canvas.point_to_cell(event.position())
        if cell is not None:
            getattr(canvas.window, add_method)(*cell)

    return handler


def _ramp_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.add_ramp(canvas.drag_start_cell, canvas.drag_current_cell, canvas.window.mode)


def _nested_map_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.drag_nested_map(canvas.drag_start_cell, canvas.drag_current_cell)


def _wall_material_tool(canvas: "Canvas", event) -> None:
    if not (canvas.drag_start_point and canvas.drag_current_point):
        return
    start, end = canvas.drag_start_point, canvas.drag_current_point
    if start == end:
        # Pure click (no drag): grab the wall under the cursor and use its
        # endpoints as the rectangle so the rect path applies to that single
        # wall.
        wall = canvas._wall_near_position(event.position())
        if wall is not None:
            start = (wall["c0"], wall["r0"])
            end = (wall["c1"], wall["r1"])
    if start != end:
        canvas.window.assign_wall_materials_rect(start, end)


def _light_tool(canvas: "Canvas", event) -> None:
    canvas.window.add_light_at(canvas.grid_position(event.position()))


def _ladder_tool(canvas: "Canvas", event) -> None:
    canvas.window.toggle_ladder_at(canvas.grid_position(event.position()))


def _erase_cells_tool(canvas: "Canvas", event) -> None:
    preserve_floors = canvas.window.mode == MODE_ERASE_KEEP_FLOORS
    if canvas.drag_start_cell and canvas.drag_current_cell and canvas.drag_start_cell != canvas.drag_current_cell:
        canvas.window.erase_cell_rect(canvas.drag_start_cell, canvas.drag_current_cell, preserve_floors)
    else:
        canvas.window.erase_at(canvas.grid_position(event.position()), preserve_floors)


CLICK_TOOLS = {
    MODE_LIGHT: _light_tool,
    MODE_LADDER: _ladder_tool,
    MODE_PRESSURE_PLATE: _click_place_tool("prompt_and_add_pressure_plate"),
    MODE_BRIDGE_PLATE: _click_place_tool("prompt_and_add_bridge_plate"),
    MODE_FIREWORK_PLATE: _click_place_tool("add_firework_plate"),
    MODE_ITEM: _click_place_tool("prompt_and_add_item"),
}


RELEASE_TOOLS = {
    MODE_FLOOR: _cell_rect_tool("add_floor_rect"),
    MODE_INACCESSIBLE_FLOOR: _cell_rect_tool("add_inaccessible_floor_rect"),
    MODE_GRASS: _cell_rect_tool("add_grass_rect"),
    MODE_ACTOR_SPAWN_ZONE: _cell_rect_tool("add_actor_spawn_zone_rect"),
    MODE_PLAYER_SPAWN_ZONE: _cell_rect_tool("add_player_spawn_zone_rect"),
    MODE_WALL: _wall_line_tool("add_wall_line"),
    MODE_BARRIER: _wall_line_tool("prompt_and_add_barrier_line"),
    MODE_EQUIPMENT_ERASER: _wall_line_tool("add_equipment_eraser_line"),
    MODE_LIGHT_BRIDGE: _cell_rect_tool("prompt_and_add_light_bridge_rect"),
    MODE_FLOOR_MATERIAL: _cell_rect_tool("assign_floor_materials_rect"),
    MODE_WALL_MATERIAL: _wall_material_tool,
    MODE_RAMP_MATERIAL: _cell_rect_tool("assign_ramp_materials_rect"),
    MODE_NESTED_MAP: _nested_map_tool,
    **dict.fromkeys(RAMP_MODES, _ramp_tool),
    **dict.fromkeys(ERASE_GROUPS, _erase_group_tool),
    **dict.fromkeys(ERASE_MODES, _erase_cells_tool),
}


class Canvas(CanvasPaintingMixin, QWidget):
    def __init__(self, window: "EditorWindow"):
        super().__init__()
        self.window = window
        self.viewport = Viewport()
        self.pan_key = False
        self.pan_origin: QPointF | None = None
        self.issue_rects: list[tuple] = []
        self.click_pending = False
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
        # Cell under the cursor while not dragging — drives the
        # `_paint_hover_ghost` overlay so paint/erase/spawn modes show what
        # the click would affect. Independent of `hover_target` (used by
        # material modes' hover-highlight pass).
        self.hover_cell: tuple[int, int] | None = None
        self.hover_grid_point: tuple[int, int] | None = None
        # Edge-picking modes (Ladder, Light): the cell side the click would
        # target, tracked continuously so the hover ghost snaps between
        # sides as the cursor moves — without it there's nothing to aim at.
        self.hover_edge_side: str | None = None
        self._hover_label = QLabel(self)
        self._hover_label.setStyleSheet(
            "background-color: rgba(15, 23, 42, 230);"
            "color: #f1f5f9;"
            "border: 1px solid #475569;"
            "border-radius: 4px;"
            "padding: 4px 8px;"
        )
        self._hover_label.hide()
        self.notice = CanvasNotice(self)
        self.setToolTip("Wheel: zoom · Space-drag or middle-drag: pan · F: fit map")
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.DefaultContextMenu)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def minimumSizeHint(self):
        return super().minimumSizeHint().expandedTo(QSize(360, 360))

    def sizeHint(self):
        cols = max(1, self.window.map_data["grid_cols"])
        rows = max(1, self.window.map_data["grid_rows"])
        return QSize(cols * EDITOR_CELL, rows * EDITOR_CELL).expandedTo(self.minimumSizeHint())

    def cell_size(self) -> float:
        return self.viewport.cell

    # A widget position in grid units, the frame every map operation works in.
    def grid_position(self, pos) -> QPointF:
        return self.viewport.to_grid(QPointF(pos))

    def resizeEvent(self, event) -> None:
        if self.viewport.fitted:
            self.fit_map()
        self.notice.reposition()
        super().resizeEvent(event)

    def fit_map(self) -> None:
        self.viewport.fit(self.width(), self.height(), self.window.map_data["grid_cols"], self.window.map_data["grid_rows"])
        self._clear_hover()
        self.update()

    def zoom_by(self, factor: float, anchor: QPointF | None = None) -> None:
        self.viewport.zoom(factor, anchor if anchor is not None else QPointF(self.width() / 2, self.height() / 2))
        self._clear_hover()
        self.update()

    def wheelEvent(self, event) -> None:
        delta = event.angleDelta().y() or event.pixelDelta().y()
        self.zoom_by(1.2 ** (delta / 120), event.position())
        event.accept()

    def keyPressEvent(self, event) -> None:
        if event.key() == Qt.Key.Key_Space:
            self.pan_key = True
            self.setCursor(Qt.CursorShape.OpenHandCursor)
            event.accept()
        else:
            super().keyPressEvent(event)

    def keyReleaseEvent(self, event) -> None:
        if event.key() == Qt.Key.Key_Space and not event.isAutoRepeat():
            self.pan_key = False
            self.setCursor(self.window.cursor_for_mode(self.window.mode))
            event.accept()
        else:
            super().keyReleaseEvent(event)

    def focusOutEvent(self, event) -> None:
        self.pan_key = False
        self.pan_origin = None
        self.setCursor(self.window.cursor_for_mode(self.window.mode))
        super().focusOutEvent(event)

    def visible_entries(self, name: str, entries: list[dict]):
        visible = self.viewport.visible_rect(self.width(), self.height()).adjusted(-1, -1, 1, 1)
        for entry in entries:
            c0, r0, c1, r1 = record_rect(name, entry)
            if visible.intersects(QRectF(c0, r0, max(.1, c1 - c0), max(.1, r1 - r0))):
                yield entry

    # A grid-unit distance for `pixels` on screen, for picking tolerances.
    def cells_per_pixel(self, pixels: float) -> float:
        return pixels / self.cell_size()

    def point_to_cell(self, pos) -> tuple[int, int] | None:
        grid = self.grid_position(pos)
        col = int(grid.x() // 1)
        row = int(grid.y() // 1)
        if 0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]:
            return col, row
        return None

    def point_to_grid_point(self, pos) -> tuple[int, int]:
        grid = self.grid_position(pos)
        col = round(grid.x())
        row = round(grid.y())
        return (
            max(0, min(self.window.map_data["grid_cols"], col)),
            max(0, min(self.window.map_data["grid_rows"], row)),
        )

    def mousePressEvent(self, event) -> None:
        if event.button() == Qt.MouseButton.MiddleButton or (self.pan_key and event.button() == Qt.MouseButton.LeftButton):
            self.window.cancel_interaction()
            self.pan_origin = event.position()
            self.setCursor(Qt.CursorShape.ClosedHandCursor)
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        self.setFocus(Qt.FocusReason.MouseFocusReason)
        self.clear_drag()
        if self.window.mode in CLICK_TOOLS:
            self.click_pending = self.point_to_cell(event.position()) is not None
            self._update_cell_hover(event.position())
            return
        if self.window.mode == MODE_SELECT and not self.window.begin_select_press(
            self.grid_position(event.position()),
            edit_objects=bool(event.modifiers() & Qt.KeyboardModifier.AltModifier),
        ):
            self.update()
            return
        self.drag_start_cell = self.point_to_cell(event.position())
        self.drag_current_cell = self.drag_start_cell
        self.drag_start_point = self.point_to_grid_point(event.position())
        self.drag_current_point = self.drag_start_point
        self.update()

    def mouseMoveEvent(self, event) -> None:
        if self.pan_origin is not None:
            self.viewport.pan(event.position() - self.pan_origin)
            self.pan_origin = event.position()
            self.update()
            return
        if self.window.mode in CLICK_TOOLS or not (event.buttons() & Qt.MouseButton.LeftButton):
            if self.window.mode in MATERIAL_MODES:
                self._update_material_hover(event.position())
            else:
                # Hover ghost for non-material modes: track which cell (and
                # grid point, for wall/barrier modes) the cursor is over so
                # `_paint_hover_ghost` can show a per-mode preview.
                self._update_cell_hover(event.position())
            return
        if self.window.mode == MODE_SELECT:
            self.window.update_select_drag(self.grid_position(event.position()))
        self.drag_current_cell = self.point_to_cell(event.position()) or self.drag_current_cell
        self.drag_current_point = self.point_to_grid_point(event.position())
        self.update()

    def leaveEvent(self, _event) -> None:
        self._clear_hover()

    # Drops every in-progress interaction: drag, hover, and pan.
    def cancel(self) -> None:
        self.clear_drag()
        self._clear_hover()
        self.pan_origin = None

    def _clear_hover(self) -> None:
        changed = self.hover_target is not None or self.hover_cell is not None or self.hover_grid_point is not None
        self.hover_kind = None
        self.hover_target = None
        self.hover_cell = None
        self.hover_grid_point = None
        self.hover_edge_side = None
        if changed:
            self.update()
        self._hover_label.hide()

    def _update_cell_hover(self, pos) -> None:
        cell = self.point_to_cell(pos)
        grid_point = self.point_to_grid_point(pos)
        edge_side = None
        if self.window.mode in (MODE_LADDER, MODE_LIGHT) and cell is not None:
            point = self.grid_position(pos)
            edge_side = cell_side_from_click(cell[0], cell[1], point.x(), point.y())
        if cell == self.hover_cell and grid_point == self.hover_grid_point and edge_side == self.hover_edge_side:
            return
        self.hover_cell = cell
        self.hover_grid_point = grid_point
        self.hover_edge_side = edge_side
        self.update()

    def _update_material_hover(self, pos) -> None:
        level_idx = self.window.current_level
        level = self.window.map_data["levels"][level_idx]

        kind: str | None = None
        target: dict | None = None
        tooltip: str | None = None

        if self.window.mode == MODE_FLOOR_MATERIAL:
            cell = self.point_to_cell(pos)
            if cell is not None:
                col, row = cell
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
        elif self.window.mode == MODE_WALL_MATERIAL:
            wall = self._wall_near_position(pos)
            if wall is not None:
                kind, target = "wall", wall
                tooltip = f"Wall\n{materials_summary(wall)}"
        else:  # MODE_RAMP_MATERIAL
            cell = self.point_to_cell(pos)
            if cell is not None:
                col, row = cell
                for ramp in self.window.map_data["ramps"]:
                    if ramp["lower_level"] == level_idx and (col, row) in ramp_cells(ramp):
                        kind, target = "ramp", ramp
                        tooltip = f"Ramp\n{materials_summary(ramp)}"
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

    def _wall_near_position(self, pos) -> dict | None:
        pos = self.grid_position(pos)
        px = pos.x()
        py = pos.y()
        level = self.window.map_data["levels"][self.window.current_level]
        for wall in level["walls"]:
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr, tolerance=0.2):
                return wall
        return None

    def mouseReleaseEvent(self, event) -> None:
        if self.pan_origin is not None:
            self.pan_origin = None
            self.setCursor(Qt.CursorShape.OpenHandCursor if self.pan_key else self.window.cursor_for_mode(self.window.mode))
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if self.click_pending:
            self.clear_drag()
            if self.point_to_cell(event.position()) is not None:
                CLICK_TOOLS[self.window.mode](self, event)
            self._update_cell_hover(event.position())
            return
        if self.drag_start_cell is None and self.drag_start_point is None and self.window.select_drag_kind is None:
            return
        if self.drag_start_cell is not None:
            self.drag_current_cell = self.point_to_cell(event.position()) or self.drag_current_cell
        if self.drag_start_point is not None:
            self.drag_current_point = self.point_to_grid_point(event.position())
        if self.window.mode == MODE_SELECT:
            self.window.end_select_drag(self.drag_start_cell, self.drag_current_cell)
        else:
            tool = RELEASE_TOOLS.get(self.window.mode)
            if tool is not None:
                tool(self, event)
        self.clear_drag()
        self.update()

    def contextMenuEvent(self, event) -> None:
        # One menu in every tool: whatever is under the cursor can be edited
        # when it has properties, and erased.
        menu = QMenu(self)
        if self.window.mode == MODE_SELECT:
            for action in (self.window.cut_action, self.window.copy_action, self.window.paste_action, self.window.delete_action):
                menu.addAction(action)
            menu.addSeparator()
        hit = self.window.hit_at(self.grid_position(event.pos()))
        preserve_floors = self.window.mode == MODE_ERASE_KEEP_FLOORS
        if hit is None:
            nothing = menu.addAction("Nothing here")
            nothing.setEnabled(False)
            menu.exec(event.globalPos())
            return
        kind, value = hit
        if kind == "Spawn Zone":
            list_name, index = value
            self.window.set_selected_spawn_zone(ZoneRef(list_name, index))
            if self.window.selected_spawn_zone_has_fields():
                menu.addAction("Edit Spawn Zone...", lambda: self.window.edit_selected_spawn_zone_fields())
        elif kind == MODE_NESTED_MAP:
            menu.addAction("Edit Nested Map...", lambda: self.window.edit_nested_map(value))
        elif kind == "Item":
            menu.addAction("Edit Item...", lambda: self.window.edit_item_at(*value))
        elif kind == "Pressure Plate":
            for plate in self.window.plates_at(*value):
                label = f"{plate['type'].capitalize()} Plate"
                if "kind" in plate:
                    label += f" ({plate['kind']})"
                    menu.addAction(
                        f"Edit {label}...",
                        lambda _checked=False, key=pressure_plate_key(plate): self.window.edit_pressure_plate_at(key),
                    )
                menu.addAction(f"Erase {label}", lambda _checked=False, key=pressure_plate_key(plate): self.window.erase_pressure_plate(key))
        if kind != "Pressure Plate" and not (preserve_floors and kind in FLOOR_HIT_KINDS):
            menu.addAction(f"Erase {kind}", lambda: self.window.erase_hit(hit, preserve_floors))
        menu.exec(event.globalPos())

    def clear_drag(self) -> None:
        self.click_pending = False
        self.drag_start_cell = None
        self.drag_current_cell = None
        self.drag_start_point = None
        self.drag_current_point = None

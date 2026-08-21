"""Map editing canvas widget."""

from __future__ import annotations


from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QLabel, QMenu, QSizePolicy, QWidget

from .constants import (
    ACTOR_ZONE_LIST,
    EDITOR_CELL,
    ERASE_MODES,
    FLOOR_HIT_KINDS,
    MATERIAL_MODES,
    MIN_CELL,
    MODE_ACTOR_SPAWN_PAINT,
    MODE_BARRIER,
    MODE_ERASE_GRASS,
    MODE_ERASE_ITEMS,
    MODE_ERASE_KEEP_FLOORS,
    MODE_ERASE_LIGHTS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_ERASE_LADDERS,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_MATERIAL,
    MODE_SPAWN_ZONE_EDIT,
    MODE_WALL,
    MODE_WALL_MATERIAL,
    PLAYER_ZONE_LIST,
    RAMP_MODES,
)
from .display import (
    materials_summary,
)
from .geometry import (
    cell_side_from_click,
    point_near_wall,
    ramp_cells,
    snapped_wall_end,
    zone_rect,
)

from .canvas_painting import CanvasPaintingMixin


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


def _wall_line_tool(method: str):
    """Tool for a grid-point drag along a wall line; the end snaps axis-aligned."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_point and canvas.drag_current_point:
            getattr(canvas.window, method)(
                canvas.drag_start_point,
                snapped_wall_end(canvas.drag_start_point, canvas.drag_current_point),
            )

    return handler


def _click_toggle_tool(remove_method: str, add_method: str):
    """Click toggles cell occupancy: an occupied cell removes; an empty cell
    prompts for a kind and places. Right-click also removes (handled in
    `contextMenuEvent`)."""

    def handler(canvas: "Canvas", event) -> None:
        if canvas.drag_start_cell:
            col, row = canvas.drag_start_cell
            if not getattr(canvas.window, remove_method)(col, row):
                getattr(canvas.window, add_method)(col, row)

    return handler


def _ramp_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell and canvas.drag_current_cell:
        canvas.window.add_ramp(canvas.drag_start_cell, canvas.drag_current_cell, canvas.window.mode)


def _spawn_zone_commit_tool(canvas: "Canvas", event) -> None:
    canvas.window.commit_spawn_zone_edit_drag()


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
    if canvas.drag_start_cell:
        canvas.window.toggle_light_at(event.position(), canvas.cell_size())


def _ladder_tool(canvas: "Canvas", event) -> None:
    if canvas.drag_start_cell:
        canvas.window.toggle_ladder_at(event.position(), canvas.cell_size())


def _erase_cells_tool(canvas: "Canvas", event) -> None:
    preserve_floors = canvas.window.mode == MODE_ERASE_KEEP_FLOORS
    if canvas.drag_start_cell and canvas.drag_current_cell and canvas.drag_start_cell != canvas.drag_current_cell:
        canvas.window.erase_cell_rect(canvas.drag_start_cell, canvas.drag_current_cell, preserve_floors)
    else:
        canvas.window.erase_at(event.position(), canvas.cell_size(), preserve_floors)


RELEASE_TOOLS = {
    MODE_FLOOR: _cell_rect_tool("add_floor_rect"),
    MODE_INACCESSIBLE_FLOOR: _cell_rect_tool("add_inaccessible_floor_rect"),
    MODE_GRASS: _cell_rect_tool("add_grass_rect"),
    MODE_ERASE_GRASS: _cell_rect_tool("erase_grass_rect"),
    MODE_ACTOR_SPAWN_PAINT: _cell_rect_tool("add_actor_spawn_zone_rect"),
    MODE_PLAYER_SPAWN_PAINT: _cell_rect_tool("add_player_spawn_zone_rect"),
    MODE_SPAWN_ZONE_EDIT: _spawn_zone_commit_tool,
    MODE_WALL: _wall_line_tool("add_wall_line"),
    MODE_BARRIER: _wall_line_tool("prompt_and_add_barrier_line"),
    MODE_FLOOR_MATERIAL: _cell_rect_tool("assign_floor_materials_rect"),
    MODE_WALL_MATERIAL: _wall_material_tool,
    MODE_RAMP_MATERIAL: _cell_rect_tool("assign_ramp_materials_rect"),
    MODE_LIGHT: _light_tool,
    MODE_LADDER: _ladder_tool,
    MODE_ERASE_LADDERS: _cell_rect_tool("erase_ladders_rect"),
    MODE_PRESSURE_PLATE: _click_toggle_tool("remove_pressure_plate_at", "prompt_and_add_pressure_plate"),
    MODE_ITEM: _click_toggle_tool("remove_item_at", "prompt_and_add_item"),
    MODE_ERASE_ITEMS: _cell_rect_tool("erase_items_rect"),
    MODE_ERASE_LIGHTS: _cell_rect_tool("erase_lights_rect"),
    **dict.fromkeys(RAMP_MODES, _ramp_tool),
    **dict.fromkeys(ERASE_MODES, _erase_cells_tool),
}


class Canvas(CanvasPaintingMixin, QWidget):
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
            if self.window.mode in MATERIAL_MODES:
                self._update_material_hover(event.position())
            else:
                # Hover ghost for non-material modes: track which cell (and
                # grid point, for wall/barrier modes) the cursor is over so
                # `_paint_hover_ghost` can show a per-mode preview.
                self._update_cell_hover(event.position())
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
            size = self.cell_size()
            edge_side = cell_side_from_click(cell[0], cell[1], pos.x() / size, pos.y() / size)
        if cell == self.hover_cell and grid_point == self.hover_grid_point and edge_side == self.hover_edge_side:
            return
        self.hover_cell = cell
        self.hover_grid_point = grid_point
        self.hover_edge_side = edge_side
        self.update()

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
        cell_size = self.cell_size()
        px = pos.x() / cell_size
        py = pos.y() / cell_size
        level = self.window.map_data["levels"][self.window.current_level]
        for wall in level["walls"]:
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr, tolerance=0.2):
                return wall
        return None

    def mouseReleaseEvent(self, event) -> None:
        if event.button() != Qt.MouseButton.LeftButton:
            return
        tool = RELEASE_TOOLS.get(self.window.mode)
        if tool is not None:
            tool(self, event)
        self.clear_drag()
        self.update()

    def contextMenuEvent(self, event) -> None:
        menu = QMenu(self)
        # Spawn-zone context menu fires in the dedicated Edit mode AND in
        # the paint modes when the click lands on a zone of the matching
        # type. Lets users edit/delete what they just painted without first
        # switching to Edit mode.
        mode_to_zone_list = {
            MODE_ACTOR_SPAWN_PAINT: ACTOR_ZONE_LIST,
            MODE_PLAYER_SPAWN_PAINT: PLAYER_ZONE_LIST,
        }
        if self.window.mode == MODE_SPAWN_ZONE_EDIT or self.window.mode in mode_to_zone_list:
            picked = self.window.spawn_zone_at(event.pos(), self.cell_size())
            # In a paint mode, only react to zones of that paint's type so
            # the menu doesn't surprise the user with unrelated zones.
            if picked is not None and self.window.mode in mode_to_zone_list:
                if picked.list_name != mode_to_zone_list[self.window.mode]:
                    picked = None
            if picked is None:
                disabled = menu.addAction("No spawn zone here")
                disabled.setEnabled(False)
            else:
                self.window.set_selected_spawn_zone(picked)
                self.update()
                if self.window.selected_spawn_zone_has_fields():
                    menu.addAction("Edit Fields...", lambda: self.window.edit_selected_spawn_zone_fields())
                menu.addAction(
                    "Delete Spawn Zone",
                    lambda: self.window.delete_selected_spawn_zone(),
                )
            menu.exec(event.globalPos())
            return
        hit = self.window.hit_at(event.pos(), self.cell_size())
        preserve_floors = self.window.mode == MODE_ERASE_KEEP_FLOORS
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            menu.addAction(
                f"Erase {hit[0]}",
                lambda: self.window.erase_hit(hit, preserve_floors),
            )
        else:
            disabled = menu.addAction("Nothing to erase")
            disabled.setEnabled(False)
        menu.exec(event.globalPos())

    def clear_drag(self) -> None:
        self.drag_start_cell = None
        self.drag_current_cell = None
        self.drag_start_point = None
        self.drag_current_point = None

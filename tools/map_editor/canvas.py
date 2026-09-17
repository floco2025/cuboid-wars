"""Map editing canvas widget."""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import QPointF, QRectF, QSize, Qt, Signal
from PySide6.QtWidgets import QLabel, QSizePolicy, QWidget

from .constants import (
    EDGE_PICK_PIXELS,
    EDITOR_CELL,
    MODE_FLOOR_MATERIAL,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_PORTAL_JUMP,
    MODE_WALL_MATERIAL,
)
from .display import materials_summary
from .hover import element_hover_text
from .geometry import (
    cell_side_from_click,
    point_near_wall,
    ramp_cells,
)

from .canvas_painting import CanvasPaintingMixin
from .interaction import CanvasInput
from .placement_input import CLICK_TOOLS
from .viewport import Viewport
from .transforms import record_rect
from .notice import CanvasNotice

if TYPE_CHECKING:
    from .window import EditorWindow


class Canvas(CanvasPaintingMixin, QWidget):
    view_changed = Signal()

    def __init__(self, window: "EditorWindow"):
        super().__init__()
        self.window = window
        self.viewport = Viewport()
        self.input = CanvasInput(self)
        self.issue_rects: list[tuple] = []
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
        # Edge-picking modes (Ladder, Light): the cell side the click would
        # target, tracked continuously so the hover ghost snaps between
        # sides as the cursor moves — without it there's nothing to aim at.
        self.hover_edge_side: str | None = None
        self._hover_label = QLabel(self)
        self._hover_label.setTextFormat(Qt.TextFormat.PlainText)
        self._hover_label.setWordWrap(True)
        self._hover_label.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._hover_label.setStyleSheet(
            "background-color: rgba(15, 23, 42, 230);"
            "color: #f1f5f9;"
            "border: 1px solid #475569;"
            "border-radius: 4px;"
            "padding: 4px 8px;"
        )
        self._hover_label.hide()
        self.notice = CanvasNotice(self)
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
        else:
            self.refresh_view()
        self.notice.reposition()
        super().resizeEvent(event)

    def fit_map(self) -> None:
        self.viewport.fit(
            self.width(), self.height(), self.window.map_data["grid_cols"], self.window.map_data["grid_rows"]
        )
        self.refresh_view()

    def zoom_by(self, factor: float, anchor: QPointF | None = None) -> None:
        self.viewport.zoom(factor, anchor if anchor is not None else QPointF(self.width() / 2, self.height() / 2))
        self.refresh_view()

    def pan_by(self, delta: QPointF) -> None:
        if delta.isNull():
            return
        origin = QPointF(self.viewport.offset)
        self.viewport.offset += delta
        self.constrain_view()
        if self.viewport.offset != origin:
            self.viewport.fitted = False
            self.refresh_view()

    def constrain_view(self) -> None:
        self.viewport.constrain(
            self.width(), self.height(), self.window.map_data["grid_cols"], self.window.map_data["grid_rows"]
        )

    def refresh_view(self) -> None:
        self.constrain_view()
        self._clear_hover()
        self.view_changed.emit()
        self.update()

    def wheelEvent(self, event) -> None:
        delta = QPointF(event.pixelDelta()) if not event.pixelDelta().isNull() else QPointF(event.angleDelta()) / 3
        if event.modifiers() & Qt.KeyboardModifier.ShiftModifier and delta.x() == 0:
            delta = QPointF(delta.y(), 0)
        if not delta.isNull():
            self.setFocus(Qt.FocusReason.MouseFocusReason)
        self.pan_by(delta)
        event.accept()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Space:
            self.input.space = True
            self.setCursor(Qt.CursorShape.OpenHandCursor)
            event.accept()
        else:
            super().keyPressEvent(event)

    def keyReleaseEvent(self, event):
        if event.key() == Qt.Key.Key_Space and not event.isAutoRepeat():
            self.input.space = False
            self.setCursor(self.window.cursor_for_mode(self.window.mode))
            event.accept()
        else:
            super().keyReleaseEvent(event)

    def focusOutEvent(self, event):
        self.input.space = False
        if self.input.gesture is not None:
            self.window.cancel_interaction()
        else:
            self.input.cancel()
        super().focusOutEvent(event)

    def visible_entries(self, name: str, entries: list[dict]):
        visible = self.viewport.visible_rect(self.width(), self.height()).adjusted(-1, -1, 1, 1)
        for entry in entries:
            c0, r0, c1, r1 = record_rect(name, entry)
            if visible.intersects(QRectF(c0, r0, max(0.1, c1 - c0), max(0.1, r1 - r0))):
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

    def mousePressEvent(self, event):
        self.input.press(event)

    def mouseMoveEvent(self, event):
        self.input.move(event)

    def leaveEvent(self, _event):
        self._clear_hover()

    def cancel(self):
        self.input.cancel()

    def _tool_point(self, end=False, *, grid=False):
        gesture = self.input.gesture
        if gesture is None or gesture.kind != "tool" or gesture.mode in CLICK_TOOLS:
            return None
        point = gesture.current if end else gesture.start
        if grid:
            return (
                max(0, min(self.window.map_data["grid_cols"], round(point.x()))),
                max(0, min(self.window.map_data["grid_rows"], round(point.y()))),
            )
        return self.input.clamped_cell(point)

    @property
    def drag_start_cell(self):
        return self._tool_point()

    @property
    def drag_current_cell(self):
        return self._tool_point(True)

    @property
    def drag_start_point(self):
        return self._tool_point(grid=True)

    @property
    def drag_current_point(self):
        return self._tool_point(True, grid=True)

    def _clear_hover(self) -> None:
        changed = self.hover_target is not None or self.hover_cell is not None
        portal_jump = getattr(self.window, "portal_jump", None)
        if portal_jump is not None and portal_jump.preview is not None:
            portal_jump.preview = None
            changed = True
        self.hover_kind = None
        self.hover_target = None
        self.hover_cell = None
        self.hover_edge_side = None
        if changed:
            self.update()
        self._hover_label.hide()

    def _update_cell_hover(self, pos) -> None:
        cell = self.point_to_cell(pos)
        portal = self.window.portal_jump
        key = portal.input_selector.currentData()
        preview = (
            portal.pick(self.grid_position(pos), key)
            if self.window.mode == MODE_PORTAL_JUMP and key in ("entry", "exit")
            else None
        )
        preview_changed = preview != portal.preview
        portal.preview = preview
        self._show_hover_label(self._element_hover_text(pos), pos)
        edge_side = None
        if self.window.mode in (MODE_LADDER, MODE_LIGHT) and cell is not None:
            point = self.grid_position(pos)
            edge_side = cell_side_from_click(cell[0], cell[1], point.x(), point.y())
        if cell == self.hover_cell and edge_side == self.hover_edge_side and not preview_changed:
            return
        self.hover_cell = cell
        self.hover_edge_side = edge_side
        self.update()

    def _element_hover_text(self, pos) -> str | None:
        hit = self.window.hit_at(self.grid_position(pos))
        return element_hover_text(self.window.map_data, self.window.current_level, hit)

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
        self._show_hover_label(tooltip or self._element_hover_text(pos), pos)

    def _show_hover_label(self, tooltip: str | None, pos) -> None:
        cell = self.point_to_cell(pos)
        guides = (self.window.jump_reach, self.window.run_time) if cell is not None else ()
        portal_text = self.window.portal_jump.hover_text(self.grid_position(pos))
        tooltip = (
            "\n".join(part for part in (tooltip, portal_text, *(guide.hover_text(*cell) for guide in guides)) if part)
            or None
        )
        if tooltip is not None:
            self._hover_label.setText(tooltip)
            metrics = self._hover_label.fontMetrics()
            width = max(metrics.horizontalAdvance(line) for line in tooltip.split("\n")) + 32
            self._hover_label.setFixedWidth(max(1, min(width, 420, self.width() - 8)))
            self._hover_label.adjustSize()
            # Offset slightly so the popup doesn't sit directly under the
            # cursor; clamp inside the canvas so it never gets clipped.
            x = int(pos.x()) + 16
            y = int(pos.y()) + 16
            if x + self._hover_label.width() > self.width() - 4:
                x = int(pos.x()) - self._hover_label.width() - 16
            if y + self._hover_label.height() > self.height() - 4:
                y = int(pos.y()) - self._hover_label.height() - 16
            x = max(0, min(x, self.width() - self._hover_label.width() - 4))
            y = max(0, min(y, self.height() - self._hover_label.height() - 4))
            self._hover_label.move(x, y)
            self._hover_label.show()
            self._hover_label.raise_()
        else:
            self._hover_label.hide()

    # How far from an edge a pick still hits it, in grid units, so edges
    # stay clickable at any zoom.
    def pick_tolerance(self) -> float:
        return self.cells_per_pixel(EDGE_PICK_PIXELS)

    def _wall_near_position(self, pos) -> dict | None:
        pos = self.grid_position(pos)
        px = pos.x()
        py = pos.y()
        tolerance = self.pick_tolerance()
        level = self.window.map_data["levels"][self.window.current_level]
        for wall in level["walls"]:
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr, tolerance):
                return wall
        return None

    def mouseReleaseEvent(self, event):
        self.input.release(event)

    def contextMenuEvent(self, event):
        self.input.context_menu(event)

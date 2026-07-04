"""Map editing canvas widget."""

from __future__ import annotations

import math

from PySide6.QtCore import QPoint, QRectF, QSize, Qt
from PySide6.QtGui import QBrush, QColor, QPainter, QPen
from PySide6.QtWidgets import QLabel, QMenu, QSizePolicy, QWidget

from .constants import (
    ACTOR_ZONE_LIST,
    BARRIER_KIND_COLORS,
    COOKIE_ZONE_LIST,
    EDITOR_CELL,
    ERASE_MODES,
    FLOOR_HIT_KINDS,
    KEY_ZONE_LIST,
    MATERIAL_MODES,
    MIN_CELL,
    MODE_ACTOR_SPAWN_PAINT,
    MODE_BARRIER,
    MODE_COOKIE_SPAWN_PAINT,
    MODE_ERASE_GRASS,
    MODE_ERASE_KEEP_FLOORS,
    MODE_ERASE_LIGHTS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_KEY_SPAWN_PAINT,
    MODE_LIGHT,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_MATERIAL,
    MODE_SPAWN_ZONE_EDIT,
    MODE_WALL,
    MODE_WALL_MATERIAL,
    PLAYER_ZONE_LIST,
    RAMP_MODES,
    SPAWN_PAINT_MODES,
    SPAWN_ZONE_HANDLE_PIXELS,
)
from .geometry import (
    BARRIER_PEN_WIDTH,
    DRAG_PREVIEW_COLORS,
    DRAG_PREVIEW_FALLBACK,
    WALL_HIGHLIGHT_WIDTH,
    WALL_PEN_WIDTH,
    draw_direction,
    face_color,
    light_marker_polygon,
    materials_summary,
    opposite_direction,
    orthogonal_arrow_points,
    point_near_wall,
    ramp_axis,
    ramp_cells,
    ramp_points_from_cells,
    ramp_rect,
    rect_from_cells,
    snapped_wall_end,
    tag_color,
    zone_color,
    zone_rect,
)


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
        # Cell under the cursor while not dragging — drives the
        # `_paint_hover_ghost` overlay so paint/erase/spawn modes show what
        # the click would affect. Independent of `hover_target` (used by
        # material modes' hover-highlight pass).
        self.hover_cell: tuple[int, int] | None = None
        self.hover_grid_point: tuple[int, int] | None = None
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

        # Painting is layered: each pass draws on top of the previous one.
        # The order here is load-bearing — moving a pass changes occlusion.
        painter.fillRect(QRectF(0, 0, cols * cell, rows * cell), QColor("#111418"))
        # Ghost the prev/next level under the current one so multi-level
        # ramps are easier to align. Floors/walls/ramps only — including
        # spawn zones and lights would clutter the view.
        if self.window.show_adjacent_levels:
            self._paint_adjacent_level_ghosts(painter, cell, level_idx)
        self._paint_floors(painter, level, cell)
        self._paint_grass(painter, level, cell)
        self._paint_pressure_plates(painter, cell, level_idx)
        self._paint_ramps(painter, cell, level_idx)
        self.paint_spawn_zones(painter, cell, level_idx)
        if self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.paint_spawn_zone_selection(painter, cell, level_idx)
        self._paint_drag_preview_rect(painter, cell)
        if self.window.mode == MODE_SPAWN_ZONE_EDIT and self.window.spawn_zone_drag is not None:
            self.paint_spawn_zone_drag_preview(painter, cell)
        self._paint_wall_and_ramp_drag_previews(painter, cell)
        self._paint_grid_lines(painter, cell, cols, rows)
        self._paint_walls(painter, level, cell)
        self._paint_barriers(painter, level, cell)
        self._paint_wall_material_drag(painter, cell)
        # Lights sit on top of wall lines so the markers stay visible.
        self.paint_lights(painter, level, cell)
        self._paint_pending_auto_lights(painter, cell, level_idx)
        # Hover passes draw last so they sit on top of everything else. The
        # highlight + ghost paths fire on disjoint mode sets and don't
        # overlap.
        self.paint_hover_highlight(painter, cell, level_idx)
        self._paint_hover_ghost(painter, cell)

    def _paint_hover_ghost(self, painter: QPainter, cell: float) -> None:
        # Show a ghost of what the click/drag would affect at the cursor. The
        # color comes from the same `DRAG_PREVIEW_COLORS` table the actual
        # drag uses, so hover-feel matches drag-feel. Skip while a drag is
        # active (the real preview is already on screen) and skip for modes
        # whose own systems already paint a hover state.
        if self.drag_start_cell is not None or self.drag_start_point is not None:
            return
        mode = self.window.mode
        if mode == MODE_SPAWN_ZONE_EDIT or mode in MATERIAL_MODES:
            return
        # Edge-based modes (Wall, Barrier): no ghost yet — the drag preview
        # is the discoverability path; a single-point ghost would only show
        # a 1px dot. Skip until we add a single-segment preview later.
        if mode in (MODE_WALL, MODE_BARRIER):
            return
        if self.hover_cell is None:
            return
        col, row = self.hover_cell
        if not (0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]):
            return
        color = DRAG_PREVIEW_COLORS.get(mode, DRAG_PREVIEW_FALLBACK)
        # Slightly dimmer than the drag preview so a static hover doesn't
        # compete with the in-progress drag visual.
        ghost = QColor(color.red(), color.green(), color.blue(), max(40, color.alpha() // 2))
        painter.setPen(QPen(QColor(color.red(), color.green(), color.blue(), 180), 2))
        painter.setBrush(ghost)
        painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))

    def _paint_floors(self, painter: QPainter, level: dict, cell: float) -> None:
        painter.setPen(Qt.PenStyle.NoPen)
        overlay = self.window.show_material_overlay
        default_floor = QColor("#454f5b")
        for floor in level["floors"]:
            col, row = floor["col"], floor["row"]
            painter.setBrush(face_color(floor) if overlay else default_floor)
            painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))
        # Blocked-floor fill is darker than walkable, with a denser cross-hatch
        # in a brighter slate so the "you can't walk here" reads at a glance.
        blocked_fill = QColor("#3a4250")
        blocked_hatch = QColor("#b8c4d4")
        for floor in level["inaccessible_floors"]:
            col, row = floor["col"], floor["row"]
            rect = QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2)
            painter.setBrush(face_color(floor) if overlay else blocked_fill)
            painter.drawRect(rect)
            painter.setPen(QPen(blocked_hatch, 2))
            painter.drawLine(rect.topLeft(), rect.bottomRight())
            painter.drawLine(rect.bottomLeft(), rect.topRight())
            # Mid-axis crosshatch for extra density.
            mid_x = rect.center().x()
            mid_y = rect.center().y()
            painter.drawLine(rect.left(), mid_y, mid_x, rect.top())
            painter.drawLine(mid_x, rect.bottom(), rect.right(), mid_y)
            painter.setPen(Qt.PenStyle.NoPen)

    # Sub-cell tuft anchors, in cell units. Fixed so tufts don't jump between
    # repaints; scattered enough to read as grass without hiding the floor's
    # material-overlay color underneath.
    _GRASS_TUFT_ANCHORS = ((0.25, 0.35), (0.65, 0.25), (0.45, 0.6), (0.2, 0.8), (0.75, 0.75))

    def _paint_grass(self, painter: QPainter, level: dict, cell: float) -> None:
        grass = level.get("grass", [])
        if not grass:
            return
        painter.setPen(QPen(QColor(132, 204, 22, 230), 2))
        blade = cell * 0.14
        for tuft in grass:
            for dx, dy in self._GRASS_TUFT_ANCHORS:
                base_x = (tuft["col"] + dx) * cell
                base_y = (tuft["row"] + dy) * cell
                painter.drawLine(base_x, base_y, base_x - blade * 0.5, base_y - blade)
                painter.drawLine(base_x, base_y, base_x, base_y - blade * 1.3)
                painter.drawLine(base_x, base_y, base_x + blade * 0.5, base_y - blade)
        painter.setPen(Qt.PenStyle.NoPen)

    def _paint_pressure_plates(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Inner 50% of the cell (≈25% by area), colored by the plate's barrier
        # kind. Matches the in-game footprint exactly.
        plates = self.window.map_data.get("pressure_plates", [])
        if not plates:
            return
        painter.setPen(Qt.PenStyle.NoPen)
        for plate in plates:
            if plate["level"] != level_idx:
                continue
            kind = plate.get("kind", "")
            hex_color = BARRIER_KIND_COLORS.get(kind, "#38bdf8")
            color = QColor(hex_color)
            painter.setBrush(color)
            inset = cell * 0.25
            painter.drawRect(QRectF(plate["col"] * cell + inset, plate["row"] * cell + inset, cell * 0.5, cell * 0.5))

    def _paint_ramps(self, painter: QPainter, cell: float, level_idx: int) -> None:
        for ramp in self.window.map_data["ramps"]:
            lower = ramp["lower_level"]
            if level_idx in (lower, lower + 1):
                self.paint_ramp(painter, ramp, cell, lower == level_idx)

    def _paint_drag_preview_rect(self, painter: QPainter, cell: float) -> None:
        if not (self.drag_start_cell and self.drag_current_cell):
            return
        if self.window.mode not in DRAG_PREVIEW_COLORS and self.window.mode not in SPAWN_PAINT_MODES:
            return
        c0, r0, c1, r1 = rect_from_cells(self.drag_start_cell, self.drag_current_cell)
        color = DRAG_PREVIEW_COLORS.get(self.window.mode, DRAG_PREVIEW_FALLBACK)
        painter.setBrush(color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))

    def _paint_wall_and_ramp_drag_previews(self, painter: QPainter, cell: float) -> None:
        if self.drag_start_point and self.drag_current_point and self.window.mode == MODE_WALL:
            end = snapped_wall_end(self.drag_start_point, self.drag_current_point)
            self.paint_wall_preview(painter, self.drag_start_point, end, cell)
        elif self.drag_start_point and self.drag_current_point and self.window.mode == MODE_BARRIER:
            end = snapped_wall_end(self.drag_start_point, self.drag_current_point)
            # The kind is asked *after* the drag, so the live preview can't
            # know it yet. Use the recently-chosen kind's color, or fall back
            # to a neutral cyan if nothing's been picked yet.
            recent = self.window.recent_barrier_kind
            hex_color = BARRIER_KIND_COLORS.get(recent, "#38bdf8")
            self.paint_wall_preview(painter, self.drag_start_point, end, cell, color=QColor(hex_color))
        elif self.drag_start_cell and self.drag_current_cell and self.window.mode in RAMP_MODES:
            self.paint_ramp_preview(painter, self.drag_start_cell, self.drag_current_cell, cell)

    def _paint_grid_lines(self, painter: QPainter, cell: float, cols: int, rows: int) -> None:
        painter.setPen(QPen(QColor("#2e343b"), 1))
        for col in range(cols + 1):
            x = col * cell
            painter.drawLine(x, 0, x, rows * cell)
        for row in range(rows + 1):
            y = row * cell
            painter.drawLine(0, y, cols * cell, y)

    def _paint_walls(self, painter: QPainter, level: dict, cell: float) -> None:
        overlay = self.window.show_material_overlay
        default_wall_color = QColor("#f1f5f9")
        for wall in level["walls"]:
            color = face_color(wall) if overlay else default_wall_color
            painter.setPen(QPen(color, WALL_PEN_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(wall["c0"] * cell, wall["r0"] * cell, wall["c1"] * cell, wall["r1"] * cell)

    def _paint_barriers(self, painter: QPainter, level: dict, cell: float) -> None:
        # Solid stroke, thinner than walls so the two read as distinct on
        # the grid. (A dashed stroke ends mid-gap on a one-cell segment,
        # which makes the line look shifted toward its start.)
        for barrier in level.get("barriers", []):
            kind = barrier.get("kind", "")
            display = BARRIER_KIND_COLORS.get(kind, "#ff5050")
            painter.setPen(QPen(QColor(display), BARRIER_PEN_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(
                barrier["c0"] * cell,
                barrier["r0"] * cell,
                barrier["c1"] * cell,
                barrier["r1"] * cell,
            )

    def _paint_wall_material_drag(self, painter: QPainter, cell: float) -> None:
        # Grid-point based: 2D rectangle when the drag spans both axes, or a
        # thick line when it collapses onto a single row or column. Painted
        # after walls so it sits on top of them.
        if not (
            self.window.mode == MODE_WALL_MATERIAL
            and self.drag_start_point
            and self.drag_current_point
        ):
            return
        sc, sr = self.drag_start_point
        ec, er = self.drag_current_point
        c0_pt, c1_pt = sorted((sc, ec))
        r0_pt, r1_pt = sorted((sr, er))
        highlight = QColor(236, 72, 153, 230)
        if c0_pt == c1_pt or r0_pt == r1_pt:
            painter.setPen(QPen(highlight, WALL_HIGHLIGHT_WIDTH, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(c0_pt * cell, r0_pt * cell, c1_pt * cell, r1_pt * cell)
        else:
            painter.setBrush(QColor(236, 72, 153, 110))
            painter.setPen(QPen(highlight, 2))
            painter.drawRect(QRectF(c0_pt * cell, r0_pt * cell, (c1_pt - c0_pt) * cell, (r1_pt - r0_pt) * cell))
        painter.setPen(Qt.PenStyle.NoPen)

    def paint_lights(self, painter: QPainter, level: dict, cell: float) -> None:
        lights = level.get("lights", [])
        if not lights:
            return
        painter.setBrush(QColor(250, 204, 21, 220))
        painter.setPen(QPen(QColor(202, 138, 4, 255), 1))
        for light in lights:
            painter.drawPolygon(light_marker_polygon(light, cell))

    def _paint_adjacent_level_ghosts(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Lower the opacity for the whole ghost pass, then restore it. Each
        # adjacent level renders with the same paint helpers as the current
        # one — keeps style consistent, just dimmer.
        painter.save()
        painter.setOpacity(0.25)
        levels = self.window.map_data["levels"]
        for offset in (-1, 1):
            target = level_idx + offset
            if 0 <= target < len(levels):
                neighbor = levels[target]
                self._paint_floors(painter, neighbor, cell)
                self._paint_ramps(painter, cell, target)
                self._paint_walls(painter, neighbor, cell)
                self._paint_barriers(painter, neighbor, cell)
        painter.restore()

    def _paint_pending_auto_lights(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Ghost overlay for the Auto-Place Lights confirmation. Cyan instead
        # of yellow so the user can tell pending-vs-committed at a glance.
        pending = self.window.pending_auto_lights
        if pending is None:
            return
        pending_level, pending_lights = pending
        if pending_level != level_idx or not pending_lights:
            return
        painter.setBrush(QColor(34, 211, 238, 160))
        painter.setPen(QPen(QColor(8, 145, 178, 255), 1, Qt.PenStyle.DashLine))
        for light in pending_lights:
            painter.drawPolygon(light_marker_polygon(light, cell))
        painter.setPen(Qt.PenStyle.NoPen)

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
        # Cookie zones first (background), then keys, then player, then actor
        # (top — has the kind label).
        for zone in self.window.map_data[COOKIE_ZONE_LIST]:
            if zone["level"] == level_idx:
                self.paint_cookie_spawn_zone(painter, zone, cell)
        for zone in self.window.map_data[KEY_ZONE_LIST]:
            if zone["level"] == level_idx:
                self.paint_key_spawn_zone(painter, zone, cell)
        for zone in self.window.map_data[PLAYER_ZONE_LIST]:
            if zone["level"] == level_idx:
                self.paint_player_spawn_zone(painter, zone, cell)
        for zone in self.window.map_data[ACTOR_ZONE_LIST]:
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

    def paint_cookie_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        rect = QRectF(c0 * cell + 2, r0 * cell + 2, (c1 - c0) * cell - 4, (r1 - r0) * cell - 4)
        outline_color = QColor(202, 138, 4)  # darker gold for outline
        fill_color = QColor(250, 204, 21, 70)  # translucent gold fill
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2, Qt.PenStyle.DotLine))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, "cookie")

    def paint_key_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        rect = QRectF(c0 * cell + 2, r0 * cell + 2, (c1 - c0) * cell - 4, (r1 - r0) * cell - 4)
        kind = zone.get("kind", "")
        hex_color = BARRIER_KIND_COLORS.get(kind, "#cccccc")
        outline_color = QColor(hex_color)
        fill_color = QColor(hex_color)
        fill_color.setAlpha(80)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2, Qt.PenStyle.DashDotLine))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, f"key {kind}")

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
        rect = QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell)
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def paint_ramp_preview(
        self,
        painter: QPainter,
        start_cell: tuple[int, int],
        end_cell: tuple[int, int],
        cell: float,
    ) -> None:
        start_point, end_point = ramp_points_from_cells(start_cell, end_cell)
        c0, r0 = min(start_point[0], end_point[0]), min(start_point[1], end_point[1])
        c1, r1 = max(start_point[0], end_point[0]), max(start_point[1], end_point[1])
        painter.setPen(QPen(QColor("#fbbf24"), 2, Qt.PenStyle.DashLine))
        painter.setBrush(QColor(217, 119, 6, 90))
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))
        direction = draw_direction(start_cell, end_cell)
        start, end = orthogonal_arrow_points(c0, r0, c1, r1, direction, cell)
        self.draw_arrow(painter, start, end, QColor("#fbbf24"))

    def paint_wall_preview(
        self,
        painter: QPainter,
        start: tuple[int, int],
        end: tuple[int, int],
        cell: float,
        color: QColor | None = None,
    ) -> None:
        pen_color = color if color is not None else QColor("#38bdf8")
        painter.setPen(QPen(pen_color, 3, Qt.PenStyle.DashLine, Qt.PenCapStyle.RoundCap))
        painter.drawLine(start[0] * cell, start[1] * cell, end[0] * cell, end[1] * cell)

    def draw_arrow(
        self,
        painter: QPainter,
        start: tuple[float, float],
        end: tuple[float, float],
        color: QColor,
    ) -> None:
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
        if changed:
            self.update()
        self._hover_label.hide()

    def _update_cell_hover(self, pos) -> None:
        cell = self.point_to_cell(pos)
        grid_point = self.point_to_grid_point(pos)
        if cell == self.hover_cell and grid_point == self.hover_grid_point:
            return
        self.hover_cell = cell
        self.hover_grid_point = grid_point
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
        if self.window.mode == MODE_FLOOR and self.drag_start_cell and self.drag_current_cell:
            self.window.add_floor_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_INACCESSIBLE_FLOOR and self.drag_start_cell and self.drag_current_cell:
            self.window.add_inaccessible_floor_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_GRASS and self.drag_start_cell and self.drag_current_cell:
            self.window.add_grass_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_ERASE_GRASS and self.drag_start_cell and self.drag_current_cell:
            self.window.erase_grass_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_ACTOR_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.add_actor_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_PLAYER_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.add_player_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_COOKIE_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.add_cookie_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_KEY_SPAWN_PAINT and self.drag_start_cell and self.drag_current_cell:
            self.window.prompt_and_add_key_spawn_zone_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_SPAWN_ZONE_EDIT:
            self.window.commit_spawn_zone_edit_drag()
        elif self.window.mode == MODE_WALL and self.drag_start_point and self.drag_current_point:
            self.window.add_wall_line(
                self.drag_start_point,
                snapped_wall_end(self.drag_start_point, self.drag_current_point),
            )
        elif self.window.mode == MODE_BARRIER and self.drag_start_point and self.drag_current_point:
            self.window.prompt_and_add_barrier_line(
                self.drag_start_point,
                snapped_wall_end(self.drag_start_point, self.drag_current_point),
            )
        elif self.window.mode in RAMP_MODES and self.drag_start_cell and self.drag_current_cell:
            self.window.add_ramp(self.drag_start_cell, self.drag_current_cell, self.window.mode)
        elif self.window.mode == MODE_FLOOR_MATERIAL and self.drag_start_cell and self.drag_current_cell:
            self.window.assign_floor_materials_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_WALL_MATERIAL and self.drag_start_point and self.drag_current_point:
            start, end = self.drag_start_point, self.drag_current_point
            if start == end:
                # Pure click (no drag): grab the wall under the cursor and use
                # its endpoints as the rectangle so the rect path applies to
                # that single wall.
                wall = self._wall_near_position(event.position())
                if wall is not None:
                    start = (wall["c0"], wall["r0"])
                    end = (wall["c1"], wall["r1"])
            if start != end:
                self.window.assign_wall_materials_rect(start, end)
        elif self.window.mode == MODE_RAMP_MATERIAL and self.drag_start_cell and self.drag_current_cell:
            self.window.assign_ramp_materials_rect(self.drag_start_cell, self.drag_current_cell)
        elif self.window.mode == MODE_LIGHT and self.drag_start_cell:
            self.window.toggle_light_at(event.position(), self.cell_size())
        elif self.window.mode == MODE_PRESSURE_PLATE and self.drag_start_cell:
            col, row = self.drag_start_cell
            # If a plate is already here, treat the click as remove; otherwise
            # prompt for kind and place. Right-click also removes (handled
            # in contextMenuEvent below).
            if not self.window.remove_pressure_plate_at(col, row):
                self.window.prompt_and_add_pressure_plate(col, row)
        elif self.window.mode == MODE_ERASE_LIGHTS and self.drag_start_cell and self.drag_current_cell:
            self.window.erase_lights_rect(self.drag_start_cell, self.drag_current_cell)
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
        # Spawn-zone context menu fires in the dedicated Edit mode AND in
        # the four paint modes when the click lands on a zone of the
        # matching type. Lets users edit/delete what they just painted
        # without first switching to Edit mode.
        mode_to_zone_list = {
            MODE_ACTOR_SPAWN_PAINT: ACTOR_ZONE_LIST,
            MODE_PLAYER_SPAWN_PAINT: PLAYER_ZONE_LIST,
            MODE_COOKIE_SPAWN_PAINT: COOKIE_ZONE_LIST,
            MODE_KEY_SPAWN_PAINT: KEY_ZONE_LIST,
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

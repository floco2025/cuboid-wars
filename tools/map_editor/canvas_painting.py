"""Canvas painting: paintEvent and every paint helper. Input handling lives in canvas.py."""

from __future__ import annotations

import math

from PySide6.QtCore import QPoint, QPointF, QRectF, Qt
from PySide6.QtGui import QBrush, QColor, QPainter, QPen

from .constants import (
    ACTOR_ZONE_LIST,
    FIREWORK_PLATE_COLOR,
    ITEMS_LIST,
    ITEM_KEY_TYPE,
    ITEM_TYPE_COLORS,
    MATERIAL_MODES,
    MODE_BARRIER,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_SELECT,
    MODE_NESTED_MAP,
    MODE_WALL,
    MODE_WALL_MATERIAL,
    PLATE_TYPE_BRIDGE,
    PLATE_TYPE_FIREWORK,
    PLAYER_ZONE_LIST,
    RAMP_MODES,
    SPAWN_ZONE_MODES,
    SPAWN_ZONE_HANDLE_PIXELS,
)
from .nested_maps import nested_map_label, nested_map_rest_points
from .normalization import ladder_spans_level, nested_map_spans_level

from .display import (
    BARRIER_PEN_WIDTH,
    DRAG_PREVIEW_COLORS,
    DRAG_PREVIEW_FALLBACK,
    NESTED_MAP_COLOR,
    WALL_HIGHLIGHT_WIDTH,
    WALL_PEN_WIDTH,
    face_color,
    tag_color,
    zone_color,
)
from .geometry import (
    draw_direction,
    ladder_anchor_from_click,
    ladder_marker_lines,
    light_marker_polygon,
    opposite_direction,
    wall_endpoints_for_cell_side,
    orthogonal_arrow_points,
    ramp_axis,
    ramp_cells,
    ramp_points_from_cells,
    ramp_rect,
    rect_from_cells,
    snapped_wall_end,
    zone_rect,
)



class CanvasPaintingMixin:
    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor("#1f2328"))
        cell = self.cell_size()
        cols = self.window.map_data["grid_cols"]
        rows = self.window.map_data["grid_rows"]
        level_idx = self.window.current_level
        level = self.window.map_data["levels"][level_idx]
        painter.translate(self.viewport.offset)

        # Painting is layered: each pass draws on top of the previous one.
        # The order here is load-bearing — moving a pass changes occlusion.
        painter.fillRect(QRectF(0, 0, cols * cell, rows * cell), QColor("#111418"))
        # Ghost the prev/next level under the current one so multi-level
        # ramps are easier to align. Floors/walls/ramps only — including
        # spawn zones and lights would clutter the view.
        if self.window.show_adjacent_levels:
            self._paint_adjacent_level_ghosts(painter, cell, level_idx)
        self._paint_floors(painter, level, cell)
        self._paint_light_bridges(painter, level, cell)
        self._paint_grass(painter, level, cell)
        self._paint_pressure_plates(painter, cell, level_idx)
        self._paint_items(painter, cell, level_idx)
        self._paint_ramps(painter, cell, level_idx)
        self.paint_spawn_zones(painter, cell, level_idx)
        if self.window.mode == MODE_SELECT:
            self.paint_spawn_zone_selection(painter, cell, level_idx)
        self._paint_drag_preview_rect(painter, cell)
        if self.window.mode == MODE_SELECT and self.window.spawn_zone_drag is not None:
            self.paint_spawn_zone_drag_preview(painter, cell)
        self._paint_wall_and_ramp_drag_previews(painter, cell)
        self._paint_grid_lines(painter, cell, cols, rows)
        self._paint_walls(painter, level, cell)
        self._paint_barriers(painter, level, cell)
        self._paint_ladders(painter, cell, level_idx)
        self._paint_nested_maps(painter, cell, level_idx)
        self._paint_wall_material_drag(painter, cell)
        # Lights sit on top of wall lines so the markers stay visible.
        self.paint_lights(painter, level, cell)
        self._paint_pending_auto_lights(painter, cell, level_idx)
        # Hover passes draw last so they sit on top of everything else. The
        # highlight + ghost paths fire on disjoint mode sets and don't
        # overlap.
        self.paint_hover_highlight(painter, cell, level_idx)
        self._paint_hover_ghost(painter, cell)
        self._paint_tile_selection(painter, cell)
        painter.setPen(QPen(QColor("#fb7185"), 3, Qt.PenStyle.DashLine))
        painter.setBrush(QColor(251, 113, 133, 45))
        for c0, r0, c1, r1 in self.issue_rects:
            painter.drawRect(QRectF(c0 * cell, r0 * cell, max(4, (c1 - c0) * cell), max(4, (r1 - r0) * cell)))

    def _paint_tile_selection(self, painter: QPainter, cell: float) -> None:
        window = self.window
        if window.mode != MODE_SELECT:
            return
        rect = window.tile_selection
        if window.select_drag_kind == "tiles" and self.drag_start_cell is not None and self.drag_current_cell is not None:
            rect = rect_from_cells(self.drag_start_cell, self.drag_current_cell)
        if rect is None:
            return
        c0, r0, c1, r1 = rect
        painter.setPen(QPen(QColor("#38bdf8"), 2))
        painter.setBrush(QColor(56, 189, 248, 55))
        inset = min(1, cell * 0.1)
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell).adjusted(inset, inset, -inset, -inset))
        block = window.tile_clipboard
        if block is None or window.select_drag_kind is not None:
            return
        width, height, levels = block["grid_cols"], block["grid_rows"], len(block["levels"])
        fits = c0 + width <= window.map_data["grid_cols"] and r0 + height <= window.map_data["grid_rows"]
        color = QColor("#86efac" if fits else "#f87171")
        painter.setPen(QPen(color, 2, Qt.PenStyle.DashLine))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        inset = min(3, cell * 0.15)
        painter.drawRect(QRectF(c0 * cell, r0 * cell, width * cell, height * cell).adjusted(inset, inset, -inset, -inset))
        text = f"Paste replaces {width} × {height} tiles · {levels} level(s)"
        if not fits:
            text += " · outside map"
        label_width = painter.fontMetrics().horizontalAdvance(text) + 16
        label_height = painter.fontMetrics().height() + 8
        ox, oy = self.viewport.offset.x(), self.viewport.offset.y()
        x = max(-ox, min(c0 * cell, self.width() - ox - label_width))
        y = max(-oy, min(r0 * cell - label_height, self.height() - oy - label_height))
        label = QRectF(x, y, label_width, label_height)
        painter.fillRect(label, QColor("#111418"))
        painter.drawText(label, Qt.AlignmentFlag.AlignCenter, text)

    def _paint_hover_ghost(self, painter: QPainter, cell: float) -> None:
        # Show a ghost of what the click/drag would affect at the cursor. The
        # color comes from the same `DRAG_PREVIEW_COLORS` table the actual
        # drag uses, so hover-feel matches drag-feel. Skip while a drag is
        # active (the real preview is already on screen) and skip for modes
        # whose own systems already paint a hover state.
        if self.drag_start_cell is not None or self.drag_start_point is not None:
            return
        mode = self.window.mode
        if mode == MODE_SELECT or mode in MATERIAL_MODES:
            return
        # Edge-based modes (Wall, Barrier): no ghost yet — the drag preview
        # is the discoverability path; a single-point ghost would only show
        # a 1px dot. Skip until we add a single-segment preview later.
        if mode in (MODE_WALL, MODE_BARRIER):
            return
        if self.hover_cell is None:
            return
        if mode == MODE_LADDER:
            # Side-aware ghost: the glyph snaps to whichever edge the click
            # would use and previews the committed result — the ladder stands
            # on the hovered edge, so the ghost sits under the cursor. Dimmer
            # than a committed ladder. No ghost when the cell across the edge
            # is off-grid (the click would be refused).
            if self.hover_edge_side is None:
                return
            col, row = self.hover_cell
            anchor_col, anchor_row, anchor_side = ladder_anchor_from_click(col, row, self.hover_edge_side)
            cols = self.window.map_data["grid_cols"]
            rows = self.window.map_data["grid_rows"]
            if not (0 <= anchor_col < cols and 0 <= anchor_row < rows):
                return
            ghost_ladder = {"col": anchor_col, "row": anchor_row, "side": anchor_side}
            painter.setPen(QPen(QColor(251, 146, 60, 150), 2, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            for x0, y0, x1, y1 in ladder_marker_lines(ghost_ladder, cell):
                painter.drawLine(round(x0), round(y0), round(x1), round(y1))
            return
        if mode == MODE_LIGHT:
            # Side-aware ghost, shown only where the click would succeed —
            # a wall on the hovered side and no ramp footprint — so valid
            # spots read at a glance while sweeping the cursor.
            if self.hover_edge_side is None:
                return
            col, row = self.hover_cell
            window = self.window
            level_idx = window.current_level
            if wall_endpoints_for_cell_side(col, row, self.hover_edge_side) not in window._wall_endpoints_for_level(
                level_idx
            ):
                return
            if (col, row) in window._ramp_cells_for_level(level_idx):
                return
            painter.setBrush(QColor(250, 204, 21, 120))
            painter.setPen(QPen(QColor(202, 138, 4, 180), 1))
            painter.drawPolygon(light_marker_polygon({"col": col, "row": row, "side": self.hover_edge_side}, cell))
            return
        col, row = self.hover_cell
        if not (0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]):
            return
        if mode == MODE_NESTED_MAP:
            self._paint_nested_map_footprint(painter, (col, row), self.window.recent_nested_map_name(), cell, dim=True)
            painter.setPen(Qt.PenStyle.NoPen)
            return
        color = DRAG_PREVIEW_COLORS.get(mode, DRAG_PREVIEW_FALLBACK)
        # Slightly dimmer than the drag preview so a static hover doesn't
        # compete with the in-progress drag visual.
        ghost = QColor(color.red(), color.green(), color.blue(), max(40, color.alpha() // 2))
        painter.setPen(QPen(QColor(color.red(), color.green(), color.blue(), 180), 2))
        painter.setBrush(ghost)
        painter.drawRect(QRectF(col * cell + 1, row * cell + 1, cell - 2, cell - 2))

    def _paint_floors(self, painter: QPainter, level: dict, cell: float) -> None:
        inset = min(1, cell * 0.1)
        painter.setPen(Qt.PenStyle.NoPen)
        overlay = self.window.show_material_overlay
        default_floor = QColor("#454f5b")
        for floor in self.visible_entries("floors", level["floors"]):
            col, row = floor["col"], floor["row"]
            painter.setBrush(face_color(floor) if overlay else default_floor)
            painter.drawRect(QRectF(col * cell + inset, row * cell + inset, cell - 2 * inset, cell - 2 * inset))
        # Blocked-floor fill is darker than walkable, with a denser cross-hatch
        # in a brighter slate so the "you can't walk here" reads at a glance.
        blocked_fill = QColor("#3a4250")
        blocked_hatch = QColor("#b8c4d4")
        for floor in self.visible_entries("inaccessible_floors", level["inaccessible_floors"]):
            col, row = floor["col"], floor["row"]
            rect = QRectF(col * cell + inset, row * cell + inset, cell - 2 * inset, cell - 2 * inset)
            painter.setBrush(face_color(floor) if overlay else blocked_fill)
            painter.drawRect(rect)
            if cell < 8:
                continue
            painter.setPen(QPen(blocked_hatch, 2))
            painter.drawLine(rect.topLeft(), rect.bottomRight())
            painter.drawLine(rect.bottomLeft(), rect.topRight())
            # Mid-axis crosshatch for extra density.
            mid_x = rect.center().x()
            mid_y = rect.center().y()
            painter.drawLine(rect.left(), mid_y, mid_x, rect.top())
            painter.drawLine(mid_x, rect.bottom(), rect.right(), mid_y)
            painter.setPen(Qt.PenStyle.NoPen)

    def _paint_light_bridges(self, painter: QPainter, level: dict, cell: float) -> None:
        # Translucent fill plus a one-way diagonal hatch, so a bridge reads as
        # a walkway that is not a floor (blocked floors cross-hatch).
        bridges = level.get("light_bridges", [])
        if not bridges:
            return
        for bridge in self.visible_entries("light_bridges", bridges):
            color = QColor(self.window.bridge_kind_colors.get(bridge.get("kind", ""), "#30d8ff"))
            inset = min(1, cell * 0.1)
            rect = QRectF(bridge["col"] * cell + inset, bridge["row"] * cell + inset, cell - 2 * inset, cell - 2 * inset)
            fill = QColor(color)
            fill.setAlpha(115)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(fill)
            painter.drawRect(rect)
            if cell < 8:
                continue
            painter.setPen(QPen(color, 1))
            size = rect.width()
            for offset in (size * 0.5, size, size * 1.5):
                if offset <= size:
                    start = QPointF(rect.left(), rect.top() + offset)
                    end = QPointF(rect.left() + offset, rect.top())
                else:
                    start = QPointF(rect.left() + offset - size, rect.bottom())
                    end = QPointF(rect.right(), rect.top() + offset - size)
                painter.drawLine(start, end)
        painter.setPen(Qt.PenStyle.NoPen)

    # Sub-cell tuft anchors, in cell units. Fixed so tufts don't jump between
    # repaints; scattered enough to read as grass without hiding the floor's
    # material-overlay color underneath.
    _GRASS_TUFT_ANCHORS = ((0.25, 0.35), (0.65, 0.25), (0.45, 0.6), (0.2, 0.8), (0.75, 0.75))

    def _paint_grass(self, painter: QPainter, level: dict, cell: float) -> None:
        if cell < 8:
            return
        grass = level.get("grass", [])
        if not grass:
            return
        painter.setPen(QPen(QColor(132, 204, 22, 230), 2))
        blade = cell * 0.14
        for tuft in self.visible_entries("grass", grass):
            for dx, dy in self._GRASS_TUFT_ANCHORS:
                base_x = (tuft["col"] + dx) * cell
                base_y = (tuft["row"] + dy) * cell
                painter.drawLine(base_x, base_y, base_x - blade * 0.5, base_y - blade)
                painter.drawLine(base_x, base_y, base_x, base_y - blade * 1.3)
                painter.drawLine(base_x, base_y, base_x + blade * 0.5, base_y - blade)
        painter.setPen(Qt.PenStyle.NoPen)

    def _paint_pressure_plates(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Inner 50% of the cell (≈25% by area) — the in-game footprint. Barrier
        # plates are squares in their kind's color, bridge plates diamonds in
        # theirs, firework plates circles in the firework color — one shape per
        # purpose so plates sharing a cell still read.
        plates = self.window.map_data.get("pressure_plates", [])
        if not plates:
            return
        painter.setPen(Qt.PenStyle.NoPen)
        for plate in self.visible_entries("pressure_plates", plates):
            if plate["level"] != level_idx:
                continue
            inset = cell * 0.25
            rect = QRectF(plate["col"] * cell + inset, plate["row"] * cell + inset, cell * 0.5, cell * 0.5)
            if plate.get("type") == PLATE_TYPE_FIREWORK:
                painter.setBrush(QColor(FIREWORK_PLATE_COLOR))
                painter.drawEllipse(rect)
            elif plate.get("type") == PLATE_TYPE_BRIDGE:
                painter.setBrush(QColor(self.window.bridge_kind_colors.get(plate.get("kind", ""), "#30d8ff")))
                cx, cy = rect.center().x(), rect.center().y()
                half = cell * 0.25
                painter.drawPolygon(
                    [
                        QPoint(round(cx), round(cy - half)),
                        QPoint(round(cx + half), round(cy)),
                        QPoint(round(cx), round(cy + half)),
                        QPoint(round(cx - half), round(cy)),
                    ]
                )
            else:
                painter.setBrush(QColor(self.window.barrier_kind_colors.get(plate.get("kind", ""), "#38bdf8")))
                painter.drawRect(rect)

    def _paint_items(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # Glyphs mirror the in-game meshes (client/src/items/spawn.rs):
        # cookie = small sphere, speed = tetrahedron, multi_shot =
        # cube, low_gravity = sphere, health potion = capsule. Keys: diamond
        # in the barrier-kind color — shape-distinct from the plates' inset
        # squares so a key on a plate cell still reads.
        items = self.window.map_data.get(ITEMS_LIST, [])
        if not items:
            return
        painter.setPen(QPen(QColor("#0f172a"), 1))
        for item in self.visible_entries("items", items):
            if item["level"] != level_idx:
                continue
            cx = (item["col"] + 0.5) * cell
            cy = (item["row"] + 0.5) * cell
            item_type = item["type"]
            if item_type == ITEM_KEY_TYPE:
                painter.setBrush(QColor(self.window.barrier_kind_colors.get(item.get("kind", ""), "#cccccc")))
                half = cell * 0.28
                painter.drawPolygon(
                    [
                        QPoint(round(cx), round(cy - half)),
                        QPoint(round(cx + half), round(cy)),
                        QPoint(round(cx), round(cy + half)),
                        QPoint(round(cx - half), round(cy)),
                    ]
                )
                continue
            painter.setBrush(QColor(ITEM_TYPE_COLORS.get(item_type, "#f8fafc")))
            if item_type == "cookie":
                # Half the power-up size, like COOKIE_SIZE vs ITEM_SIZE.
                radius = cell * 0.13
                painter.drawEllipse(QRectF(cx - radius, cy - radius, radius * 2, radius * 2))
            elif item_type == "speed":
                half = cell * 0.26
                painter.drawPolygon(
                    [
                        QPoint(round(cx), round(cy - half)),
                        QPoint(round(cx + half), round(cy + half)),
                        QPoint(round(cx - half), round(cy + half)),
                    ]
                )
            elif item_type == "multi_shot":
                half = cell * 0.22
                painter.drawRect(QRectF(cx - half, cy - half, half * 2, half * 2))
            elif item_type == "health_potion":
                half_w = cell * 0.16
                half_h = cell * 0.30
                painter.drawRoundedRect(QRectF(cx - half_w, cy - half_h, half_w * 2, half_h * 2), half_w, half_w)
            elif item_type == "missile_pack":
                # Rocket silhouette: slim body rect + nose triangle.
                half_w = cell * 0.10
                half_h = cell * 0.24
                painter.drawRect(QRectF(cx - half_w, cy - half_h * 0.4, half_w * 2, half_h * 1.4))
                painter.drawPolygon(
                    [
                        QPoint(round(cx), round(cy - half_h)),
                        QPoint(round(cx + half_w), round(cy - half_h * 0.4)),
                        QPoint(round(cx - half_w), round(cy - half_h * 0.4)),
                    ]
                )
            else:  # low_gravity — sphere
                radius = cell * 0.24
                painter.drawEllipse(QRectF(cx - radius, cy - radius, radius * 2, radius * 2))
        painter.setPen(Qt.PenStyle.NoPen)

    def _paint_ramps(self, painter: QPainter, cell: float, level_idx: int) -> None:
        for ramp in self.visible_entries("ramps", self.window.map_data["ramps"]):
            lower = ramp["lower_level"]
            if level_idx in (lower, lower + 1):
                self.paint_ramp(painter, ramp, cell, lower == level_idx)

    def _paint_drag_preview_rect(self, painter: QPainter, cell: float) -> None:
        if not (self.drag_start_cell and self.drag_current_cell):
            return
        if self.window.mode not in DRAG_PREVIEW_COLORS and self.window.mode not in SPAWN_ZONE_MODES:
            return
        # A nested map drag is two ends and a band, not a rectangle.
        if self.window.mode == MODE_NESTED_MAP:
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
            hex_color = self.window.barrier_kind_colors.get(recent, "#38bdf8")
            self.paint_wall_preview(painter, self.drag_start_point, end, cell, color=QColor(hex_color))
        elif self.drag_start_cell and self.drag_current_cell and self.window.mode in RAMP_MODES:
            self.paint_ramp_preview(painter, self.drag_start_cell, self.drag_current_cell, cell)
        elif self.drag_start_cell and self.drag_current_cell and (
            self.window.mode == MODE_NESTED_MAP or self.window.select_drag_kind == "nested"
        ):
            self._paint_nested_map_drag(painter, cell)

    def _paint_grid_lines(self, painter: QPainter, cell: float, cols: int, rows: int) -> None:
        if cell < 4:
            return
        painter.setPen(QPen(QColor("#2e343b"), 1))
        visible = self.viewport.visible_rect(self.width(), self.height())
        for col in range(max(0, math.floor(visible.left())), min(cols, math.ceil(visible.right())) + 1):
            x = col * cell
            painter.drawLine(x, 0, x, rows * cell)
        for row in range(max(0, math.floor(visible.top())), min(rows, math.ceil(visible.bottom())) + 1):
            y = row * cell
            painter.drawLine(0, y, cols * cell, y)

    def _paint_walls(self, painter: QPainter, level: dict, cell: float) -> None:
        overlay = self.window.show_material_overlay
        default_wall_color = QColor("#f1f5f9")
        for wall in self.visible_entries("walls", level["walls"]):
            color = face_color(wall) if overlay else default_wall_color
            painter.setPen(QPen(color, min(WALL_PEN_WIDTH, max(1, cell * 0.2)), Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(wall["c0"] * cell, wall["r0"] * cell, wall["c1"] * cell, wall["r1"] * cell)

    def _paint_barriers(self, painter: QPainter, level: dict, cell: float) -> None:
        # Solid stroke, thinner than walls so the two read as distinct on
        # the grid. (A dashed stroke ends mid-gap on a one-cell segment,
        # which makes the line look shifted toward its start.)
        for barrier in self.visible_entries("barriers", level.get("barriers", [])):
            kind = barrier.get("kind", "")
            display = self.window.barrier_kind_colors.get(kind, "#ff5050")
            painter.setPen(QPen(QColor(display), min(BARRIER_PEN_WIDTH, max(1, cell * 0.15)), Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
            painter.drawLine(
                barrier["c0"] * cell,
                barrier["r0"] * cell,
                barrier["c1"] * cell,
                barrier["r1"] * cell,
            )

    def _paint_ladders(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # A ladder paints on every level it passes through (like ramps on
        # both of theirs), so multi-storey spans stay visible while editing
        # any level along the way.
        ladders = [l for l in self.window.map_data.get("ladders", []) if ladder_spans_level(l, level_idx)]
        if not ladders:
            return
        painter.setPen(QPen(QColor("#fb923c"), 2, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        for ladder in self.visible_entries("ladders", ladders):
            for x0, y0, x1, y1 in ladder_marker_lines(ladder, cell):
                painter.drawLine(round(x0), round(y0), round(x1), round(y1))

    def paint_motion_span(
        self,
        painter: QPainter,
        start: tuple[int, int],
        end: tuple[int, int],
        start_level: int,
        end_level: int,
        level_idx: int,
        cell: float,
        storeys: int,
        dim: bool = False,
    ) -> None:
        # One language: a square at each end of the nested map's travel,
        # numbered 1 (where it rests at phase zero) and 2, a band over the
        # cells its anchor sweeps between them, and a hollow square for an
        # end on another storey. It goes back and forth, so nothing points
        # one way. An end is "here" on any of the map's own `storeys` above
        # the storey it rests on.
        color = NESTED_MAP_COLOR
        inset = cell * 0.12
        tile = cell - 2 * inset
        scale = 0.5 if dim else 1.0
        line = QColor(color)
        line.setAlpha(int(230 * scale))
        fill = QColor(color)
        fill.setAlpha(int(160 * scale))
        band = QColor(color)
        band.setAlpha(int(70 * scale))
        center = lambda pos: ((pos[0] + 0.5) * cell, (pos[1] + 0.5) * cell)
        if end != start:
            (sx, sy), (ex, ey) = center(start), center(end)
            painter.save()
            painter.translate(sx, sy)
            painter.rotate(math.degrees(math.atan2(ey - sy, ex - sx)))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(band)
            painter.drawRect(QRectF(0.0, -tile / 2, math.hypot(ex - sx, ey - sy), tile))
            painter.restore()

        if end != start:
            ends = [(start, start_level, "1"), (end, end_level, "2")]
        elif start_level == level_idx:
            ends = [(start, start_level, "1")]
        elif end_level == level_idx:
            ends = [(end, end_level, "2")]
        else:
            ends = [(start, level_idx + 1, "")]
        for pos, level, number in ends:
            here = level <= level_idx < level + storeys
            cx, cy = center(pos)
            rect = QRectF(cx - tile / 2, cy - tile / 2, tile, tile)
            painter.setPen(QPen(line, 2))
            painter.setBrush(fill if here else Qt.BrushStyle.NoBrush)
            painter.drawRect(rect)
            if number and cell >= 12:
                painter.setPen(QColor("#ffffff") if here else line)
                painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, number)
        painter.setPen(Qt.PenStyle.NoPen)

    def _paint_nested_maps(self, painter: QPainter, cell: float, level_idx: int) -> None:
        # A nested map paints on every storey it reaches: the storeys its
        # ends rest on plus its own, so the whole building is visible from
        # each floor it passes.
        for entry in self.window.map_data.get("nested_maps", []):
            shape = self.window.nested_map_shape(entry["map"])
            storeys = shape.level_count if shape else 1
            if not nested_map_spans_level(entry, level_idx, storeys):
                continue
            start, end = tuple(entry["from"]), tuple(entry["to"])
            self.paint_motion_span(painter, start, end, entry["level"], entry["to_level"], level_idx, cell, storeys)
            # The footprints sit where the map rests: each anchor nudged.
            rest_start, rest_end = nested_map_rest_points(entry, self.window.wall_width_cells)
            name = entry["map"]
            self._paint_nested_map_footprint(
                painter, rest_start, name, cell, label=nested_map_label(name, entry["from_nudge"])
            )
            if end != start or rest_end != rest_start:
                self._paint_nested_map_footprint(
                    painter, rest_end, name, cell, dashed=True, label=nested_map_label(name, entry["to_nudge"])
                )

    def _paint_nested_map_footprint(
        self,
        painter: QPainter,
        anchor: tuple[float, float],
        name: str | None,
        cell: float,
        dashed: bool = False,
        dim: bool = False,
        label: str | None = None,
    ) -> None:
        # The nested map's grid with its cell (0, 0) on the anchor, outlined
        # solid where it starts and dashed where it arrives, named in the
        # middle. An unknown map is a red single cell asking to be fixed.
        shape = self.window.nested_map_shape(name) if name else None
        if shape is None:
            cols, rows, color, label = 1, 1, QColor(248, 113, 113), f"{name or '?'}?"
        else:
            cols, rows, color = shape.grid_cols, shape.grid_rows, QColor(NESTED_MAP_COLOR)
            label = label or name
        color.setAlpha(120 if dim else 230)
        rect = QRectF(anchor[0] * cell + 2, anchor[1] * cell + 2, cols * cell - 4, rows * cell - 4)
        pen = QPen(color, 2)
        if dashed:
            pen.setStyle(Qt.PenStyle.DashLine)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRect(rect)
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)
        painter.setPen(Qt.PenStyle.NoPen)

    def _paint_nested_map_drag(self, painter: QPainter, cell: float) -> None:
        # A drag from an existing end previews that end at the cursor with
        # the other end where it is; any other drag previews a new entry.
        start, current = self.drag_start_cell, self.drag_current_cell
        level_idx = self.window.current_level
        hit = self.window.nested_map_end_at(start)
        if hit is None:
            name = self.window.recent_nested_map_name()
            self.paint_motion_span(painter, start, current, level_idx, level_idx, level_idx, cell, 1, dim=True)
            self._paint_nested_map_footprint(painter, current, name, cell, dashed=current != start, dim=True)
            return
        entry, end = hit
        moved = {**entry, end: [current[0], current[1]]}
        self.paint_motion_span(
            painter,
            tuple(moved["from"]),
            tuple(moved["to"]),
            entry["level"],
            entry["to_level"],
            level_idx,
            cell,
            1,
            dim=True,
        )
        self._paint_nested_map_footprint(painter, current, entry["map"], cell, dashed=end == "to", dim=True)

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
        for light in self.visible_entries("lights", lights):
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
                self._paint_light_bridges(painter, neighbor, cell)
                self._paint_nested_maps(painter, cell, target)
                self._paint_ramps(painter, cell, target)
                self._paint_walls(painter, neighbor, cell)
                self._paint_barriers(painter, neighbor, cell)
                self._paint_ladders(painter, cell, target)
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
        # Player zones first (background), then actor (top — has the kind label).
        for zone in self.visible_entries(PLAYER_ZONE_LIST, self.window.map_data[PLAYER_ZONE_LIST]):
            if zone["level"] == level_idx:
                self.paint_player_spawn_zone(painter, zone, cell)
        for zone in self.visible_entries(ACTOR_ZONE_LIST, self.window.map_data[ACTOR_ZONE_LIST]):
            if zone["level"] == level_idx:
                self.paint_actor_spawn_zone(painter, zone, cell)

    def paint_actor_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        inset = min(2, cell * 0.1)
        rect = QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell).adjusted(inset, inset, -inset, -inset)
        outline_color = zone_color(zone["kind"])
        fill_color = QColor(outline_color)
        fill_color.setAlpha(70)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        label = f"{zone['kind']}:{zone['count']}" if zone["kind"] else "(empty)"
        if cell >= 8:
            painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def paint_player_spawn_zone(self, painter: QPainter, zone: dict, cell: float) -> None:
        c0, r0, c1, r1 = zone_rect(zone)
        inset = min(2, cell * 0.1)
        rect = QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell).adjusted(inset, inset, -inset, -inset)
        outline_color = tag_color("player")
        fill_color = QColor(outline_color)
        fill_color.setAlpha(70)
        painter.setBrush(QBrush(fill_color))
        painter.setPen(QPen(outline_color, 2, Qt.PenStyle.DashLine))
        painter.drawRect(rect)
        painter.setPen(QColor("#f8fafc"))
        if cell >= 8:
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

    def paint_ramp(self, painter: QPainter, ramp: dict, cell: float, is_lower_level: bool) -> None:
        c0, r0, c1, r1 = ramp_rect(ramp)
        painter.setPen(QPen(QColor("#111827"), 1))
        if self.window.show_material_overlay:
            painter.setBrush(face_color(ramp))
        else:
            painter.setBrush(QColor("#d97706") if is_lower_level else QColor("#8b5cf6"))
        inset = min(3, cell * 0.15)
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell).adjusted(inset, inset, -inset, -inset))
        if cell < 8:
            return

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

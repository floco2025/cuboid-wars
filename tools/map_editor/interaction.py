"""One active gesture routes canvas input to selection or placement."""

import copy
from dataclasses import dataclass

from PySide6.QtCore import QPointF, Qt
from PySide6.QtWidgets import QApplication, QMenu

from . import constants as c
from .elements import ElementRef, element_refs, refs_for_hit, refs_in_region, refs_on_grid_line
from .geometry import rect_from_cells, zone_handle_centers
from .nesting import nested_map_footprints, nested_map_label
from .normalization import nested_map_spans_level, normalize_map
from .placement_input import CLICK_TOOLS, RELEASE_TOOLS
from .regions import TileRegion
from .selection import Selection
from .selection_transfer import snapped_offset
from .spawn_zones import resized_zone_rect
from .transforms import EDGE_LISTS, record_levels


@dataclass(frozen=True)
class Handle:
    ref: ElementRef
    name: str
    point: tuple[float, float]


@dataclass
class Gesture:
    kind: str
    start: QPointF
    current: QPointF
    initial: Selection
    mode: str
    handle: Handle | None = None
    original: dict | None = None
    moved: bool = False
    object_type: str | None = None
    hits: tuple[ElementRef, ...] = ()


class CanvasInput:
    def __init__(self, canvas):
        self.canvas = canvas
        self.window = canvas.window
        self.gesture = None
        self.space = False

    def cell(self, point):
        col, row = int(point.x() // 1), int(point.y() // 1)
        if 0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]:
            return col, row
        return None

    def within_grid(self, point, tolerance=None):
        if tolerance is None:
            tolerance = self.canvas.cells_per_pixel(1)
        return (
            -tolerance <= point.x() <= self.window.map_data["grid_cols"] + tolerance
            and -tolerance <= point.y() <= self.window.map_data["grid_rows"] + tolerance
        )

    def tool_accepts(self, mode, point):
        if mode == c.MODE_PORTAL_JUMP:
            return self.within_grid(point, tolerance=self.canvas.pick_tolerance())
        if mode in (c.MODE_WALL, c.MODE_BARRIER, c.MODE_EQUIPMENT_ERASER, c.MODE_WALL_MATERIAL):
            return self.within_grid(point)
        return self.cell(point) is not None

    def handles(self):
        refs = self.window.selection.objects
        if len(refs) != 1 or self.window.mode != c.MODE_SELECT:
            return []
        ref = refs[0]
        entry = ref.get(self.window.map_data)
        lower, upper = record_levels(entry, ref.level)
        if not lower <= self.window.current_level <= upper:
            return []
        if ref.name in c.ZONE_LISTS:
            return [
                Handle(ref, name, point)
                for name, point in zip(("nw", "n", "ne", "e", "se", "s", "sw", "w"), zone_handle_centers(entry))
            ]
        if ref.name == "nested_maps":
            return [
                Handle(ref, end, (entry[end][0] + 0.5, entry[end][1] + 0.5))
                for end, level in (("from", "level"), ("to", "to_level"))
                if entry[level] == self.window.current_level
            ]
        return []

    def handle_at(self, point):
        radius = self.canvas.cells_per_pixel(max(c.SPAWN_ZONE_HANDLE_PIXELS * 0.75, 6))
        return next(
            (
                handle
                for handle in self.handles()
                if abs(point.x() - handle.point[0]) <= radius and abs(point.y() - handle.point[1]) <= radius
            ),
            None,
        )

    def hits(self, point, *, alternate=False, additive=False):
        if alternate:
            zone = self.window.spawn_zone_at(point)
            if zone is not None:
                return [ElementRef(zone.list_name, zone.index)]
            cell = self.cell(point)
            if cell is not None:
                for ref, entry in element_refs(self.window.map_data):
                    if ref.name == "nested_maps" and any(
                        entry[end] == list(cell) and entry[level] == self.window.current_level
                        for end, level in (("from", "level"), ("to", "to_level"))
                    ):
                        return [ref]
        hit = self.window.hit_at(point)
        # A nested map's outline and label rank with its anchor cells in
        # `hit_at`: above the floors, below everything else, so what stands
        # on its edge stays clickable. Its selected footprint is a drag
        # handle, not something a Shift-click toggles.
        if hit is None or hit[0] in (c.HIT_FLOOR, c.HIT_INACCESSIBLE_FLOOR, c.HIT_TERRAIN):
            nested = self.nested_map_at(point, selected_footprint=not additive)
            if nested is not None:
                return [nested]
        return refs_for_hit(self.window.map_data, self.window.current_level, hit)

    def nested_map_at(self, point, *, selected_footprint):
        tolerance = self.canvas.pick_tolerance()
        entries = self.window.map_data.get("nested_maps", [])
        for index in reversed(range(len(entries))):
            entry = entries[index]
            shape = self.window.nested_map_shape(entry["map"])
            if not nested_map_spans_level(entry, self.window.current_level, shape.level_count if shape else 1):
                continue
            ref = ElementRef("nested_maps", index)
            nudges = (entry["from_nudge"], entry["to_nudge"])
            for footprint, nudge in zip(nested_map_footprints(entry, shape, self.window.wall_width_cells), nudges):
                x0, y0, x1, y1 = footprint
                if not (
                    x0 - tolerance <= point.x() <= x1 + tolerance and y0 - tolerance <= point.y() <= y1 + tolerance
                ):
                    continue
                on_outline = (
                    min(abs(point.x() - x0), abs(point.x() - x1), abs(point.y() - y0), abs(point.y() - y1)) <= tolerance
                )
                label = nested_map_label(entry["map"], nudge, known=shape is not None)
                on_label = self.canvas.nested_map_label_rect(footprint, label).contains(point)
                if on_outline or on_label or (selected_footprint and ref in self.window.selection.objects):
                    return ref
        return None

    def target(self, point, hits):
        if not self.window.selection.contains(point, hits):
            if hits:
                self.window.inspect_refs(hits)
            else:
                self.window.set_selection(Selection(anchor=self.cell(point)))
        return self.window.selection_refs()

    def press(self, event):
        button = event.button()
        if button not in (Qt.MouseButton.LeftButton, Qt.MouseButton.MiddleButton):
            return
        self.canvas.setFocus(Qt.FocusReason.MouseFocusReason)
        self.cancel()
        if button == Qt.MouseButton.MiddleButton or self.space:
            self.window.cancel_interaction()
            self.gesture = Gesture("pan", event.position(), event.position(), self.window.selection, self.window.mode)
            self.canvas.setCursor(Qt.CursorShape.ClosedHandCursor)
            return
        point = self.canvas.grid_position(event.position())
        initial = self.window.selection
        if self.window.mode != c.MODE_SELECT:
            if self.tool_accepts(self.window.mode, point):
                self.gesture = Gesture("tool", point, point, initial, self.window.mode)
                self.hover(event.position())
            return
        self.canvas._clear_hover()
        if self.window.pending_block is not None:
            self.gesture = Gesture("place_transfer", point, point, initial, c.MODE_SELECT)
            self.window.move_pending_block(point)
            return
        handle = self.handle_at(point)
        alternate = bool(event.modifiers() & Qt.KeyboardModifier.AltModifier)
        additive = bool(event.modifiers() & Qt.KeyboardModifier.ShiftModifier)
        if handle is not None and not additive:
            self.gesture = Gesture(
                "handle",
                point,
                point,
                initial,
                c.MODE_SELECT,
                handle,
                copy.deepcopy(handle.ref.get(self.window.map_data)),
            )
        elif self.within_grid(point):
            hits = self.hits(point, alternate=alternate, additive=additive)
            if self.window.selection_kind == "Tiles" and not alternate:
                if self.window.selection.area is not None and self.window.selection.contains(point, hits):
                    kind = "move"
                else:
                    self.window.set_tile_selection(rect_from_cells(self.clamped_cell(point), self.clamped_cell(point)))
                    kind = "box"
            elif additive:
                # A Shift-click toggles on release; a Shift-drag adds the
                # group its box covers, so the pressed object stays as it was.
                kind = "add_box"
            elif hits:
                kind = "move" if initial.contains(point, hits) or hits[0].name == "nested_maps" else "box"
                self.target(point, hits)
            else:
                self.window.set_selection(Selection(anchor=self.cell(point)))
                kind = "box"
            self.gesture = Gesture(
                kind,
                point,
                point,
                initial,
                c.MODE_SELECT,
                object_type=hits[0].name if hits and self.window.selection_kind == "Objects" else None,
                hits=tuple(hits),
            )
        self.canvas.update()

    def move(self, event):
        gesture = self.gesture
        if gesture is not None and gesture.kind == "pan":
            self.canvas.pan_by(event.position() - gesture.current)
            gesture.current = event.position()
            return
        point = self.canvas.grid_position(event.position())
        if gesture is None:
            if self.window.pending_block is not None:
                self.window.move_pending_block(point)
            self.hover(event.position())
            return
        gesture.current = point
        gesture.moved |= (
            point - gesture.start
        ).manhattanLength() * self.canvas.cell_size() >= QApplication.startDragDistance()
        if gesture.kind == "move" and gesture.moved:
            if self.window.pending_block is None:
                if snapped_offset(point.x() - gesture.start.x(), point.y() - gesture.start.y()) == (0, 0):
                    return
                if not self.window.begin_transfer(point=gesture.start):
                    gesture.kind = "blocked"
                    return
            self.window.move_pending_block(point)
        elif gesture.kind in ("box", "add_box") and gesture.moved and self.window.selection_kind == "Objects":
            self.select_dragged_objects()
        elif gesture.kind == "place_transfer":
            self.window.move_pending_block(point)
        elif gesture.kind == "tool" and gesture.mode in CLICK_TOOLS:
            self.hover(event.position())
        self.canvas.update()

    def release(self, event):
        gesture = self.gesture
        if gesture is None:
            return
        if gesture.kind == "pan":
            if event.button() in (Qt.MouseButton.MiddleButton, Qt.MouseButton.LeftButton):
                self.cancel()
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        self.move(event)
        if gesture.kind == "tool":
            # Range tools clamp their drag to the grid; a click released outside it is dropped.
            if gesture.mode in CLICK_TOOLS:
                if self.tool_accepts(gesture.mode, gesture.current):
                    CLICK_TOOLS[gesture.mode](self.canvas, event)
            elif gesture.mode in RELEASE_TOOLS:
                RELEASE_TOOLS[gesture.mode](self.canvas, event)
        elif gesture.kind == "add_box" and not gesture.moved:
            if gesture.hits:
                chosen = list(gesture.initial.objects)
                for ref in gesture.hits:
                    if ref in chosen:
                        chosen.remove(ref)
                    else:
                        chosen.append(ref)
                self.window.inspect_refs(chosen)
        elif gesture.kind in ("box", "add_box") and gesture.moved and self.window.selection_kind == "Tiles":
            self.window.set_tile_selection(
                rect_from_cells(self.clamped_cell(gesture.start), self.clamped_cell(gesture.current))
            )
        elif gesture.kind in ("move", "place_transfer") and self.window.pending_block is not None:
            pending = self.window.pending_block
            if gesture.kind == "move" and pending.destination == pending.source.rect[:2]:
                self.window.pending_block = None
            else:
                self.window.commit_pending_block()
        elif gesture.kind == "handle" and gesture.moved:
            self.commit_handle(gesture)
        self.gesture = None
        self.canvas.update()

    def line_selection(self):
        gesture = self.gesture
        if (
            gesture is None
            or gesture.kind not in ("box", "add_box")
            or not gesture.moved
            or self.window.selection_kind != "Objects"
            or gesture.object_type not in (None, "floors", "inaccessible_floors", "terrain", *EDGE_LISTS)
        ):
            return None
        start, end = gesture.start, gesture.current
        vertical = abs(end.y() - start.y()) > abs(end.x() - start.x())
        coordinate = round(start.x() if vertical else start.y())
        tolerance = min(0.25, self.canvas.pick_tolerance())
        if any(
            abs(value - coordinate) > tolerance
            for value in ((start.x(), end.x()) if vertical else (start.y(), end.y()))
        ):
            return None
        line = (
            (coordinate, start.y(), coordinate, end.y()) if vertical else (start.x(), coordinate, end.x(), coordinate)
        )
        refs = refs_on_grid_line(self.window.map_data, line, self.window.current_level, self.window.selection_levels)
        if not refs:
            return None
        name = gesture.object_type
        if not any(ref.name == name for ref in refs):
            origin = start.y() if vertical else start.x()

            def distance(ref):
                entry = ref.get(self.window.map_data)
                a, b = (entry["r0"], entry["r1"]) if vertical else (entry["c0"], entry["c1"])
                return max(min(a, b) - origin, origin - max(a, b), 0)

            name = min(refs, key=distance).name
        return line, [ref for ref in refs if ref.name == name]

    def select_dragged_objects(self):
        gesture = self.gesture
        line = self.line_selection()
        if line is not None:
            refs = line[1]
        else:
            region = TileRegion(
                rect_from_cells(self.clamped_cell(gesture.start), self.clamped_cell(gesture.current)),
                self.window.current_level,
                self.window.selection_levels,
            )
            refs = [
                ref
                for ref in refs_in_region(self.window.map_data, region)
                if gesture.object_type is None or ref.name == gesture.object_type
            ]
        if gesture.kind == "add_box":
            refs = [*gesture.initial.objects, *refs]
        self.window.inspect_refs(refs)

    def clamped_cell(self, point):
        return (
            max(0, min(self.window.map_data["grid_cols"] - 1, int(point.x() // 1))),
            max(0, min(self.window.map_data["grid_rows"] - 1, int(point.y() // 1))),
        )

    def zone_preview(self):
        gesture = self.gesture
        if gesture is None or gesture.kind != "handle" or gesture.handle.ref.name not in c.ZONE_LISTS:
            return None
        return resized_zone_rect(
            gesture.original,
            gesture.handle.name,
            gesture.start,
            gesture.current,
            self.window.map_data["grid_cols"],
            self.window.map_data["grid_rows"],
        )

    def commit_handle(self, gesture):
        window = self.window
        after = copy.deepcopy(window.map_data)
        entry = gesture.handle.ref.get(after)
        if gesture.handle.ref.name in c.ZONE_LISTS:
            c0, r0, c1, r1 = self.zone_preview()
            entry["cols"], entry["rows"] = [c0, c1], [r0, r1]
            label = "Resize Zone"
        else:
            dx, dy = self.clamped_cell(gesture.current)
            entry[gesture.handle.name] = [dx, dy]
            label = "Move Nested Map End"
        expected = gesture.handle.ref.get(normalize_map(after))
        if window.apply_object_change(label, after):
            if gesture.handle.ref.name in c.ZONE_LISTS:
                window.set_selected_spawn_zone(window._zone_ref_after_change(gesture.handle.ref.name, expected))
            else:
                window.inspect_refs(
                    [
                        ref
                        for ref, value in element_refs(window.map_data)
                        if ref.name == gesture.handle.ref.name
                        and ref.level == gesture.handle.ref.level
                        and value == expected
                    ]
                )

    def hover(self, position):
        if self.window.mode == c.MODE_SELECT and not self.space:
            handle = self.handle_at(self.canvas.grid_position(position))
            cursor = {
                "nw": Qt.CursorShape.SizeFDiagCursor,
                "se": Qt.CursorShape.SizeFDiagCursor,
                "ne": Qt.CursorShape.SizeBDiagCursor,
                "sw": Qt.CursorShape.SizeBDiagCursor,
                "n": Qt.CursorShape.SizeVerCursor,
                "s": Qt.CursorShape.SizeVerCursor,
                "e": Qt.CursorShape.SizeHorCursor,
                "w": Qt.CursorShape.SizeHorCursor,
                "from": Qt.CursorShape.SizeAllCursor,
                "to": Qt.CursorShape.SizeAllCursor,
            }.get(handle.name if handle else None, Qt.CursorShape.ArrowCursor)
            self.canvas.setCursor(cursor)
        if self.window.mode in c.MATERIAL_MODES:
            self.canvas._update_material_hover(position)
        else:
            self.canvas._update_cell_hover(position)

    # A cancelled press leaves the selection it found, not the one it began.
    def cancel(self):
        gesture = self.gesture
        self.gesture = None
        if gesture is not None and gesture.kind in ("box", "add_box", "move", "blocked"):
            self.window.set_selection(gesture.initial)
        self.canvas._clear_hover()
        self.canvas.setCursor(
            Qt.CursorShape.OpenHandCursor if self.space else self.window.cursor_for_mode(self.window.mode)
        )
        self.canvas.update()

    def context_menu(self, event):
        window = self.window
        point = self.canvas.grid_position(event.pos())
        if window.mode != c.MODE_SELECT:
            window.set_mode(c.MODE_SELECT)
        self.cancel()
        menu = QMenu(self.canvas)
        if window.pending_block is not None:
            for action in (window.rotate_action, window.mirror_x_action, window.mirror_y_action):
                menu.addAction(action)
            menu.exec(event.globalPos())
            return
        refs = self.target(point, self.hits(point))
        if refs or window.selection.area is not None:
            for action in (window.cut_action, window.copy_action, window.paste_action, window.delete_action):
                menu.addAction(action)
            menu.addSeparator()
            for action in (
                window.duplicate_action,
                window.rotate_action,
                window.mirror_x_action,
                window.mirror_y_action,
            ):
                menu.addAction(action)
            if len(refs) == 1:
                menu.addSeparator()
                menu.addAction("Use This Tool", lambda: window.sample_ref(refs[0]))
            elif refs and all(ref.name == "pressure_plates" for ref in refs):
                plates = [ref.get(window.map_data) for ref in refs]
                if len({(plate["level"], plate["col"], plate["row"]) for plate in plates}) == 1:
                    choose = menu.addMenu("Select plate")
                    for ref, plate in zip(refs, plates):
                        choose.addAction(
                            f"Pressure Plate ({plate.get('switch') or '?'})",
                            lambda checked=False, ref=ref: window.inspect_refs([ref]),
                        )
        elif window.paste_action.isEnabled():
            menu.addAction(window.paste_action)
        if menu.actions():
            menu.exec(event.globalPos())

"""Preview and commit moves, duplicates, rotations, and reflections."""

import copy
import math
from dataclasses import dataclass, field

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QPen

from .block_transforms import transform_block
from .checkpoint_names import name_checkpoint_copies
from .object_selection import block_fits, copy_objects, selected_data, paste_objects, refs_for_block
from .constants import MODE_SELECT
from .regions import copy_region, delete_region, nested_map_ends_inside, paste_region
from .transforms import record_levels, record_lists, translate_entry
from .selection_painting import paint_outline, paint_caption


# Half a cell rounds away from the origin, whatever its sign.
def snapped_offset(dx, dy):
    return tuple(int(math.copysign(math.floor(abs(value) + 0.5), value)) for value in (dx, dy))


@dataclass
class BlockTransfer:
    block: dict
    source: object
    duplicate: bool
    destination: tuple[int, int]
    refs: tuple | None = None
    drag_origin: tuple[float, float] | None = None
    dragging: bool = False
    additions: dict = field(default_factory=dict)


class SelectionTransferMixin:
    def begin_transfer(self, duplicate=False, point=None):
        region = self.selection_region()
        if region is None:
            return False
        try:
            refs = tuple(self.selection_refs()) if self.selection.area is None else None
            if refs is not None:
                if not refs:
                    self.notify("No objects selected.")
                    return False
                block, region = copy_objects(self.map_data, refs, self.definitions)
            else:
                block = copy_region(self.map_data, region)
                if not duplicate:
                    delete_region(self.map_data, region)
        except ValueError as error:
            self.notify(str(error))
            return False
        if duplicate:
            block = name_checkpoint_copies(block, self.map_data)
        self.pending_block = BlockTransfer(block, region, duplicate, region.rect[:2], refs)
        if point is not None:
            self.pending_block.dragging = True
            self.pending_block.drag_origin = (point.x(), point.y())
        self.canvas.update()
        return True

    # The nested maps a pending move takes along, by index: the selected
    # ones, or for a tile area the ones it holds whole, since one crossing
    # its edge stays put.
    def moving_nested_maps(self):
        pending = self.pending_block
        if pending is None or pending.duplicate:
            return set()
        if pending.refs is not None:
            return {ref.index for ref in pending.refs if ref.name == "nested_maps"}
        return {
            index
            for index, entry in enumerate(self.map_data.get("nested_maps", []))
            if all(nested_map_ends_inside(entry, pending.source))
        }

    def duplicate_selection(self):
        if self.pending_block is not None:
            self.notify("Place or cancel the pending selection first")
            return
        if self.begin_transfer(duplicate=True):
            self.notify("Place duplicate · Esc cancels")
            self.canvas.setFocus()

    def transform_selection(self, operation):
        had_pending = self.pending_block is not None
        if self.pending_block is None and not self.begin_transfer():
            return
        pending = self.pending_block
        try:
            definitions = self.doc.nested_geometry | pending.additions
            pending.block, additions = transform_block(pending.block, operation, definitions)
            pending.additions.update(additions)
            pending.dragging = False
            pending.drag_origin = None
        except ValueError as error:
            if not had_pending:
                self.pending_block = None
            self.notify(str(error))
            return
        self.notify("Place transformed selection · Esc cancels")
        self.canvas.update()
        self.canvas.setFocus()

    def move_pending_block(self, point):
        if self.pending_block is None:
            return
        pending = self.pending_block
        if pending.drag_origin is not None:
            ox, oy = pending.drag_origin
            col, row = pending.source.rect[:2]
            dc, dr = snapped_offset(point.x() - ox, point.y() - oy)
            pending.destination = (col + dc, row + dr)
        else:
            pending.destination = (int(point.x() // 1), int(point.y() // 1))
        self.canvas.update()

    def commit_pending_block(self):
        pending = self.pending_block
        if pending is None:
            return
        region = pending.source
        try:
            if pending.refs is not None:
                data = self.map_data if pending.duplicate else selected_data(self.map_data, pending.refs, remove=True)
                after = paste_objects(data, pending.block, pending.destination, region.level)
                if not pending.additions:
                    errors = self.added_issues(after)
                else:
                    # Validate before maintenance can remove an unselected
                    # dependent, using the transformed geometry definitions.
                    candidate = copy.deepcopy(self.doc.root_data)
                    if self.doc.active_map is None:
                        candidate = copy.deepcopy(after)
                    else:
                        candidate["nested_geometry"][self.doc.active_map] = after
                    candidate.setdefault("nested_geometry", {}).update(pending.additions)
                    errors = self.added_document_issues(candidate)
                if errors:
                    raise ValueError(errors[0])
            else:
                data = self.map_data
                if not pending.duplicate:
                    data = delete_region(data, region)
                after = paste_region(data, pending.block, pending.destination, region.level)
            after = self.doc.maintain(after)
            if self.doc.active_map is None:
                root = after
            else:
                root = copy.deepcopy(self.doc.root_data)
                root["nested_geometry"][self.doc.active_map] = after
            # Repeated preview transforms can leave intermediate definitions;
            # only definitions reached by the final block belong in the map.
            required = set()

            def collect(data):
                for entry in data.get("nested_maps", []):
                    name = entry["map"]
                    if name in pending.additions and name not in required:
                        required.add(name)
                        collect(pending.additions[name])

            collect(pending.block)
            if required:
                root.setdefault("nested_geometry", {}).update({name: pending.additions[name] for name in required})
            errors = self.added_document_issues(root)
            if errors:
                raise ValueError(errors[0])
            self.pending_block = None
            self.doc.apply_root_change(
                "Duplicate Selection" if pending.duplicate else "Transform Selection", root, self.doc.active_map
            )
        except ValueError as error:
            self.notify(str(error))
            return
        col, row = pending.destination
        self.pending_block = None
        if pending.refs is not None:
            self.inspect_refs(refs_for_block(self.map_data, pending.block, pending.destination, region.level))
        else:
            self.selection_levels = len(pending.block["levels"])
            self.set_tile_selection((col, row, col + pending.block["grid_cols"], row + pending.block["grid_rows"]))

    def paint_transfer(self, painter, cell):
        pending = self.pending_block
        if pending is None or self.mode != MODE_SELECT:
            return
        col, row = pending.destination
        width, height = pending.block["grid_cols"], pending.block["grid_rows"]
        if pending.refs is None:
            fits = (
                0 <= col
                and 0 <= row
                and col + width <= self.map_data["grid_cols"]
                and row + height <= self.map_data["grid_rows"]
            )
        else:
            fits = block_fits(pending.block, pending.destination, self.map_data)
        color = QColor("#86efac" if fits else "#f87171")
        painter.save()
        painter.setPen(QPen(color, 2, Qt.PenStyle.DashLine))
        painter.setBrush(QColor(color.red(), color.green(), color.blue(), 40))
        if pending.refs is None:
            painter.drawRect(QRectF(col * cell, row * cell, width * cell, height * cell))
        else:
            painter.setBrush(Qt.BrushStyle.NoBrush)
        for (level, name), entries in record_lists(pending.block):
            for entry in entries:
                if name == "nested_maps":
                    self.canvas.paint_nested_map(
                        painter,
                        translate_entry(name, entry, col, row, pending.source.level),
                        cell,
                        self.current_level,
                        color=color,
                    )
                    continue
                lower, upper = record_levels(entry, level)
                if not lower <= self.current_level - pending.source.level <= upper:
                    continue
                paint_outline(painter, name, entry, cell, (col, row))
        text = "Duplicate" if pending.duplicate else "Move / transform"
        text += " objects" if pending.refs is not None else " replaces tiles"
        text += f" · {len(pending.block['levels'])} level(s)" + (" · outside map" if not fits else "")
        paint_caption(self.canvas, painter, text)
        painter.restore()

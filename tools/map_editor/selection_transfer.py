"""Preview and commit moves, duplicates, rotations, and reflections."""

import copy
from dataclasses import dataclass, field

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QPen

from .block_transforms import transform_block
from .constants import MODE_SELECT
from .elements import restore_excluded
from .regions import copy_region, delete_region, paste_region
from .transforms import record_levels, record_lists, record_rect


@dataclass
class BlockTransfer:
    block: dict
    source: object
    duplicate: bool
    destination: tuple[int, int]
    grab_offset: tuple[int, int] = (0, 0)
    dragging: bool = False
    additions: dict = field(default_factory=dict)


class SelectionTransferMixin:
    def begin_transfer(self, duplicate=False, point=None):
        region = self.selection_region()
        if region is None:
            return False
        try:
            block = copy_region(self.editable_map_data(), region)
            if not duplicate:
                delete_region(self.editable_map_data(), region)
        except ValueError as error:
            self.notify(str(error))
            return False
        self.pending_block = BlockTransfer(block, region, duplicate, region.rect[:2])
        if point is not None:
            self.pending_block.dragging = True
            self.pending_block.grab_offset = (
                int(point.x() // 1) - region.rect[0],
                int(point.y() // 1) - region.rect[1],
            )
        self.canvas.update()
        return True

    def duplicate_selection(self):
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
            pending.grab_offset = (0, 0)
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
        ox, oy = self.pending_block.grab_offset
        self.pending_block.destination = (int(point.x() // 1) - ox, int(point.y() // 1) - oy)
        self.canvas.update()

    def commit_pending_block(self):
        pending = self.pending_block
        if pending is None:
            return
        region = pending.source
        try:
            data = self.editable_map_data()
            if not pending.duplicate:
                data = delete_region(data, region)
            after = paste_region(data, pending.block, pending.destination, region.level)
            after = restore_excluded(self.map_data, after, self.element_filters.excluded)
            # Generated definitions are validated with the completed root below.
            after = self.protect_change(after, validate_dependencies=False)
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
            before = {issue.identity() for issue in self.validate_document(self.doc.root_data).issues}
            errors = [issue.message for issue in self.validate_document(root).issues if issue.identity() not in before]
            if errors:
                raise ValueError(errors[0])
            self.doc.apply_root_change(
                "Duplicate Selection" if pending.duplicate else "Transform Selection", root, self.doc.active_map
            )
        except ValueError as error:
            self.notify(str(error))
            return
        col, row = pending.destination
        self.pending_block = None
        self.set_tile_selection((col, row, col + pending.block["grid_cols"], row + pending.block["grid_rows"]))
        self.selection_levels = len(pending.block["levels"])
        self.refresh_inspection()

    def paint_transfer(self, painter, cell):
        pending = self.pending_block
        if pending is None or self.mode != MODE_SELECT:
            return
        col, row = pending.destination
        width, height = pending.block["grid_cols"], pending.block["grid_rows"]
        fits = (
            0 <= col
            and 0 <= row
            and col + width <= self.map_data["grid_cols"]
            and row + height <= self.map_data["grid_rows"]
        )
        color = QColor("#86efac" if fits else "#f87171")
        painter.save()
        painter.setPen(QPen(color, 2, Qt.PenStyle.DashLine))
        painter.setBrush(QColor(color.red(), color.green(), color.blue(), 40))
        painter.drawRect(QRectF(col * cell, row * cell, width * cell, height * cell))
        for (level, name), entries in record_lists(pending.block):
            for entry in entries:
                lower, upper = record_levels(entry, level)
                if not lower <= self.current_level - pending.source.level <= upper:
                    continue
                c0, r0, c1, r1 = record_rect(name, entry)
                painter.drawRect(
                    QRectF((col + c0) * cell, (row + r0) * cell, max(2, (c1 - c0) * cell), max(2, (r1 - r0) * cell))
                )
        text = "Duplicate replaces" if pending.duplicate else "Move / transform replaces"
        text += f" · {len(pending.block['levels'])} level(s)" + (" · outside map" if not fits else "")
        canvas = self.canvas
        x = max(-canvas.viewport.offset.x(), col * cell)
        y = max(-canvas.viewport.offset.y(), row * cell - 24)
        rect = QRectF(x, y, painter.fontMetrics().horizontalAdvance(text) + 12, 24)
        painter.fillRect(rect, QColor("#111418"))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, text)
        painter.restore()

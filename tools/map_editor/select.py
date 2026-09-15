"""Tile selection, clipboard actions, and object drags."""

from __future__ import annotations

import json

from PySide6.QtCore import QMimeData
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication

from .constants import MODE_SELECT
from .geometry import rect_from_cells
from .normalization import normalize_map, nested_map_key
from .regions import copy_region, delete_region, paste_region
from .elements import filtered_map, restore_excluded
from .types import DRAG_NESTED_END, DRAG_SPAWN_ZONE, DRAG_TILES, DRAG_BLOCK


CLIPBOARD_MIME = "application/x-cuboid-wars-map-block+json"


class SelectMixin:
    def build_selection_actions(self, menu) -> None:
        self.cut_action = self.add_menu_action(menu, "Cu&t", QKeySequence.StandardKey.Cut, self.cut_selection)
        self.copy_action = self.add_menu_action(menu, "&Copy", QKeySequence.StandardKey.Copy, self.copy_selection)
        self.paste_action = self.add_menu_action(menu, "&Paste", QKeySequence.StandardKey.Paste, self.paste_selection)
        self.delete_action = self.add_menu_action(menu, "&Delete", None, self.delete_selection)
        self.delete_action.setShortcuts([QKeySequence("Delete"), QKeySequence("Backspace")])
        self.edit_barriers_action = self.add_menu_action(
            menu, "Edit Selected Barriers…", None, lambda: self.edit_selected_fields("barriers")
        )
        self.edit_bridges_action = self.add_menu_action(
            menu, "Edit Selected Light Bridges…", None, lambda: self.edit_selected_fields("light_bridges")
        )
        self.duplicate_action = self.add_menu_action(
            menu, "Duplicate Selection", QKeySequence("Ctrl+D"), self.duplicate_selection
        )
        self.rotate_action = self.add_menu_action(
            menu, "Rotate Selection Clockwise", None, lambda: self.transform_selection("rotate")
        )
        self.mirror_x_action = self.add_menu_action(
            menu, "Mirror Selection Horizontally", None, lambda: self.transform_selection("mirror_x")
        )
        self.mirror_y_action = self.add_menu_action(
            menu, "Mirror Selection Vertically", None, lambda: self.transform_selection("mirror_y")
        )
        menu.addSeparator()
        self.add_menu_action(menu, "Select &All Tiles", QKeySequence.StandardKey.SelectAll, self.select_all_tiles)
        self.deselect_action = self.add_menu_action(menu, "Deselect", QKeySequence("Escape"), self.clear_selection)
        QApplication.clipboard().dataChanged.connect(self.read_tile_clipboard)
        self.read_tile_clipboard()

    def read_tile_clipboard(self) -> None:
        mime = QApplication.clipboard().mimeData()
        self.tile_clipboard = None
        if mime is not None and mime.hasFormat(CLIPBOARD_MIME):
            try:
                data = json.loads(bytes(mime.data(CLIPBOARD_MIME)))["map"]
                block = normalize_map(data)
                if block["grid_cols"] > 0 and block["grid_rows"] > 0:
                    self.tile_clipboard = block
            except (ValueError, TypeError, KeyError, IndexError, AttributeError, OverflowError):
                pass
        self.update_selection_actions()
        self.canvas.update()

    def update_selection_actions(self) -> None:
        selected = self.mode == MODE_SELECT and self.tile_selection is not None
        for action in (
            self.cut_action,
            self.copy_action,
            self.delete_action,
            self.edit_barriers_action,
            self.edit_bridges_action,
            self.duplicate_action,
            self.rotate_action,
            self.mirror_x_action,
            self.mirror_y_action,
        ):
            action.setEnabled(selected)
        self.paste_action.setEnabled(selected and self.tile_clipboard is not None)
        self.deselect_action.setEnabled(True)
        self.tool_settings.sync_values()

    def set_tile_selection(self, rect: tuple[int, int, int, int] | None) -> None:
        self.tile_selection = rect
        self.inspected_refs = []
        self.selected_spawn_zone_ref = None
        self.update_selection_actions()
        self.refresh_inspection()
        self.canvas.update()

    def clear_selection(self) -> None:
        self.cancel_interaction()
        self.set_tile_selection(None)

    def cancel_interaction(self) -> None:
        if self.pending_block is not None:
            self.notify("Pending selection cancelled")
        self.spawn_zone_drag = None
        self.select_drag_kind = None
        self.pending_block = None
        self.canvas.cancel()

    def select_all_tiles(self) -> None:
        self.set_mode(MODE_SELECT)
        self.set_tile_selection((0, 0, self.map_data["grid_cols"], self.map_data["grid_rows"]))

    # `pos` is in grid units.
    def begin_select_press(self, pos, *, edit_objects: bool = False) -> bool:
        self.select_drag_kind = None
        self.selection_point = pos
        self.press_point = pos
        if self.pending_block is not None:
            self.move_pending_block(pos)
            self.select_drag_kind = DRAG_BLOCK
            return True
        if self.tile_selection is not None and not edit_objects:
            c0, r0, c1, r1 = self.tile_selection
            if c0 <= pos.x() < c1 and r0 <= pos.y() < r1:
                # The transfer starts once the pointer leaves the pressed
                # cell, so a click inspects instead of lifting the block.
                self.select_drag_kind = DRAG_BLOCK
                return True
        if edit_objects or self.selected_spawn_zone_handle(pos) is not None:
            self.tile_selection = None
            if self.begin_spawn_zone_drag(pos):
                self.select_drag_kind = DRAG_SPAWN_ZONE
                self.update_selection_actions()
                return False
            cell = (int(pos.x() // 1), int(pos.y() // 1))
            if self.nested_map_end_at(cell) is not None:
                self.select_drag_kind = DRAG_NESTED_END
                self.update_selection_actions()
                return True
        self.set_tile_selection(None)
        self.select_drag_kind = DRAG_TILES
        self.update_selection_actions()
        return True

    def update_select_drag(self, pos) -> None:
        self.selection_point = pos
        if self.select_drag_kind == DRAG_BLOCK:
            if self.pending_block is None:
                pressed = self.press_point
                if (int(pos.x() // 1), int(pos.y() // 1)) == (int(pressed.x() // 1), int(pressed.y() // 1)):
                    return
                if not self.begin_transfer(point=pressed):
                    self.select_drag_kind = None
                    return
            self.move_pending_block(pos)
        elif self.select_drag_kind == DRAG_SPAWN_ZONE:
            self.update_spawn_zone_edit_drag(pos)

    def end_select_drag(self, start_cell: tuple[int, int] | None, end_cell: tuple[int, int] | None) -> None:
        kind = self.select_drag_kind
        self.select_drag_kind = None
        if kind == DRAG_BLOCK:
            pending = self.pending_block
            if pending is None:
                self.inspect_hit(self.hit_at(self.selection_point))
            elif pending.dragging and pending.destination == pending.source.rect[:2]:
                self.pending_block = None
                self.inspect_hit(self.hit_at(self.selection_point))
            else:
                self.commit_pending_block()
        elif kind == DRAG_SPAWN_ZONE:
            self.commit_spawn_zone_edit_drag()
        elif start_cell is not None and end_cell is not None:
            if kind == DRAG_TILES:
                self.set_tile_selection(rect_from_cells(start_cell, end_cell))
                if start_cell == end_cell:
                    self.inspect_hit(self.hit_at(self.selection_point))
                else:
                    self.refresh_inspection(show=True)
            elif kind == DRAG_NESTED_END and end_cell != start_cell:
                hit = self.nested_map_end_at(start_cell)
                if hit is not None:
                    entry, end = hit
                    self.move_nested_map_end(nested_map_key(entry), end, end_cell)
        self.update_selection_actions()

    def _selection_region(self, operation):
        return self.selection_region() if self.mode == MODE_SELECT else None

    def copy_selection(self) -> None:
        self._edit_selection("Copy", copy_tiles=True, delete_tiles=False)

    def cut_selection(self) -> None:
        self._edit_selection("Cut", copy_tiles=True, delete_tiles=True)

    def delete_selection(self) -> None:
        self._edit_selection("Delete", copy_tiles=False, delete_tiles=True)

    def _edit_selection(self, operation: str, *, copy_tiles: bool, delete_tiles: bool) -> None:
        region = self._selection_region(operation)
        if region is None:
            return
        try:
            block = copy_region(self.editable_map_data(), region) if copy_tiles else None
            after = delete_region(self.editable_map_data(), region) if delete_tiles else None
            if after is not None:
                after = restore_excluded(self.map_data, after, self.element_filters.excluded)
        except ValueError as exc:
            self.notify(f"Cannot {operation.lower()}: {exc}")
            return
        if after is not None and not self.apply_change(f"{operation} Tiles ({region.levels} level(s))", after):
            return
        if block is not None:
            mime = QMimeData()
            mime.setData(CLIPBOARD_MIME, json.dumps({"map": block}).encode("utf-8"))
            QApplication.clipboard().setMimeData(mime)
        if after is None:
            self.notify(f"Copied tiles from {region.levels} level(s)")

    def paste_selection(self) -> None:
        if self.mode != MODE_SELECT or self.tile_selection is None or self.tile_clipboard is None:
            return
        col, row = self.tile_selection[:2]
        try:
            block = filtered_map(self.tile_clipboard, self.element_filters.excluded)
            after = paste_region(self.editable_map_data(), block, (col, row), self.current_level)
            after = restore_excluded(self.map_data, after, self.element_filters.excluded)
            before = {issue.identity() for issue in self.validate(self.map_data).issues}
            added_errors = [issue.message for issue in self.validate(after).issues if issue.identity() not in before]
            if added_errors:
                raise ValueError("The pasted block conflicts with the destination:\n\n" + "\n".join(added_errors[:8]))
        except ValueError as exc:
            self.notify(f"Cannot paste: {exc}")
            return
        if not self.apply_change("Paste Tiles", after):
            return
        self.set_tile_selection(
            (col, row, col + self.tile_clipboard["grid_cols"], row + self.tile_clipboard["grid_rows"])
        )

"""Tile selection, clipboard actions, and object drags."""

from __future__ import annotations

import json

from PySide6.QtCore import QMimeData
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication, QInputDialog, QMessageBox

from .constants import MODE_SELECT
from .geometry import rect_from_cells
from .normalization import canonicalize_map, nested_map_key
from .regions import TileRegion, copy_region, delete_region, paste_region
from .validation import validate_map


CLIPBOARD_MIME = "application/x-cuboid-wars-map-block+json"


class SelectMixin:
    def build_selection_actions(self, menu) -> None:
        self.cut_action = self.add_menu_action(menu, "Cu&t...", QKeySequence.StandardKey.Cut, self.cut_selection)
        self.copy_action = self.add_menu_action(menu, "&Copy...", QKeySequence.StandardKey.Copy, self.copy_selection)
        self.paste_action = self.add_menu_action(menu, "&Paste", QKeySequence.StandardKey.Paste, self.paste_selection)
        self.delete_action = self.add_menu_action(menu, "&Delete...", None, self.delete_selection)
        self.delete_action.setShortcuts([QKeySequence("Delete"), QKeySequence("Backspace")])
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
                block = canonicalize_map(data)
                if block["grid_cols"] > 0 and block["grid_rows"] > 0:
                    self.tile_clipboard = block
            except (ValueError, TypeError, KeyError, IndexError, AttributeError, OverflowError):
                pass
        self.update_selection_actions()
        self.canvas.update()

    def update_selection_actions(self) -> None:
        selected = self.mode == MODE_SELECT and self.tile_selection is not None
        for action in (self.cut_action, self.copy_action, self.delete_action):
            action.setEnabled(selected)
        self.paste_action.setEnabled(selected and self.tile_clipboard is not None)
        self.deselect_action.setEnabled(True)

    def set_tile_selection(self, rect: tuple[int, int, int, int] | None) -> None:
        self.tile_selection = rect
        self.selected_spawn_zone_ref = None
        self.update_selection_actions()
        self.canvas.update()

    def clear_selection(self) -> None:
        self.cancel_interaction()
        self.set_tile_selection(None)

    def cancel_interaction(self) -> None:
        self.spawn_zone_drag = None
        self.select_drag_kind = None
        self.canvas.clear_drag()
        self.canvas._clear_hover()

    def select_all_tiles(self) -> None:
        self.mode_combo.setCurrentText(MODE_SELECT)
        self.set_tile_selection((0, 0, self.map_data["grid_cols"], self.map_data["grid_rows"]))

    def begin_select_press(self, pos, cell_size: float, *, edit_objects: bool = False) -> bool:
        self.select_drag_kind = None
        if edit_objects:
            self.tile_selection = None
            if self.begin_spawn_zone_drag(pos, cell_size):
                self.select_drag_kind = "zone"
                self.update_selection_actions()
                return False
            cell = (int(pos.x() // cell_size), int(pos.y() // cell_size))
            if self.nested_map_end_at(cell) is not None:
                self.select_drag_kind = "nested"
                self.update_selection_actions()
                return True
        self.set_tile_selection(None)
        self.select_drag_kind = "tiles"
        self.update_selection_actions()
        return True

    def update_select_drag(self, pos, cell_size: float) -> None:
        if self.select_drag_kind == "zone":
            self.update_spawn_zone_edit_drag(pos, cell_size)

    def end_select_drag(self, start_cell: tuple[int, int] | None, end_cell: tuple[int, int] | None) -> None:
        kind = self.select_drag_kind
        self.select_drag_kind = None
        if kind == "zone":
            self.commit_spawn_zone_edit_drag()
        elif start_cell is not None and end_cell is not None:
            if kind == "tiles":
                self.set_tile_selection(rect_from_cells(start_cell, end_cell))
            elif kind == "nested" and end_cell != start_cell:
                hit = self.nested_map_end_at(start_cell)
                if hit is not None:
                    entry, end = hit
                    self.move_nested_map_end(nested_map_key(entry), end, end_cell)
        self.update_selection_actions()

    def _selection_region(self, operation: str) -> TileRegion | None:
        if self.mode != MODE_SELECT or self.tile_selection is None:
            return None
        count, accepted = QInputDialog.getInt(
            self, f"{operation} Tiles", f"How many levels to {operation.lower()}?\nStarting at the current level, upward:",
            1, 1, len(self.map_data["levels"]) - self.current_level,
        )
        return TileRegion(self.tile_selection, self.current_level, count) if accepted else None

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
            block = copy_region(self.map_data, region) if copy_tiles else None
            after = delete_region(self.map_data, region) if delete_tiles else None
        except ValueError as exc:
            QMessageBox.information(self, f"Cannot {operation} Tiles", str(exc))
            return
        if block is not None:
            mime = QMimeData()
            mime.setData(CLIPBOARD_MIME, json.dumps({"map": block}).encode("utf-8"))
            QApplication.clipboard().setMimeData(mime)
        if after is not None:
            self.apply_change(f"{operation} Tiles ({region.levels} level(s))", after)
        else:
            self._set_last_action(f"Copied tiles from {region.levels} level(s)")

    def paste_selection(self) -> None:
        if self.mode != MODE_SELECT or self.tile_selection is None or self.tile_clipboard is None:
            return
        col, row = self.tile_selection[:2]
        try:
            self.forget_nested_map_shapes()
            after = paste_region(self.map_data, self.tile_clipboard, (col, row), self.current_level)
            errors = validate_map(
                self.tile_clipboard, self.barrier_kinds, self.bridge_kinds,
                map_name=self.edited_map_name(), nested_lookup=self.nested_map_shape,
            )
            if errors:
                raise ValueError("The copied block cannot be used in this map:\n\n" + "\n".join(errors[:8]))
            before_errors = set(validate_map(self.map_data, self.barrier_kinds, self.bridge_kinds))
            added_errors = [error for error in validate_map(after, self.barrier_kinds, self.bridge_kinds) if error not in before_errors]
            if added_errors:
                raise ValueError("The pasted block conflicts with the destination:\n\n" + "\n".join(added_errors[:8]))
        except ValueError as exc:
            QMessageBox.information(self, "Cannot Paste Tiles", str(exc))
            return
        self.apply_change("Paste Tiles", after)
        self.set_tile_selection((col, row, col + self.tile_clipboard["grid_cols"], row + self.tile_clipboard["grid_rows"]))

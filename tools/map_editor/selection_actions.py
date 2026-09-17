"""Commands operate on the published selection, never on a second target."""

from __future__ import annotations

import json

from .object_selection import copy_objects, selected_data, paste_objects, refs_for_block

from PySide6.QtCore import QMimeData
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication

from .checkpoint_numbers import number_checkpoint_copies
from .constants import MODE_SELECT
from .normalization import normalize_map
from .regions import copy_region, delete_region, paste_region


CLIPBOARD_MIME = "application/x-cuboid-wars-map-block+json"


class SelectionActionsMixin:
    def build_selection_actions(self, menu) -> None:
        self.cut_action = self.add_menu_action(menu, "Cu&t", QKeySequence.StandardKey.Cut, self.cut_selection)
        self.copy_action = self.add_menu_action(menu, "&Copy", QKeySequence.StandardKey.Copy, self.copy_selection)
        self.paste_action = self.add_menu_action(menu, "&Paste", QKeySequence.StandardKey.Paste, self.paste_selection)
        self.delete_action = self.add_menu_action(menu, "&Delete", None, self.delete_selection)
        self.delete_action.setShortcuts([QKeySequence("Delete"), QKeySequence("Backspace")])
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
        all_action = self.add_menu_action(
            menu, "Select &All", QKeySequence.StandardKey.SelectAll, self.select_all_tiles
        )
        self.canvas_shortcut(all_action)
        self.deselect_action = self.add_menu_action(menu, "Deselect", QKeySequence("Escape"), self.escape_selection)
        for action in (
            self.cut_action,
            self.copy_action,
            self.paste_action,
            self.delete_action,
            self.duplicate_action,
            self.deselect_action,
        ):
            self.canvas_shortcut(action)
        QApplication.clipboard().dataChanged.connect(self.read_tile_clipboard)
        self.read_tile_clipboard()

    def read_tile_clipboard(self) -> None:
        mime = QApplication.clipboard().mimeData()
        self.tile_clipboard = None
        self.clipboard_objects = False
        self.clipboard_view_offset = 0
        if mime is not None and mime.hasFormat(CLIPBOARD_MIME):
            try:
                payload = json.loads(bytes(mime.data(CLIPBOARD_MIME)))
                data = payload["map"]
                self.clipboard_objects = payload.get("objects") is True
                self.clipboard_view_offset = int(payload.get("view_offset", 0))
                block = normalize_map(data)
                if block["grid_cols"] > 0 and block["grid_rows"] > 0:
                    self.tile_clipboard = block
            except (ValueError, TypeError, KeyError, IndexError, AttributeError, OverflowError):
                pass
        self.update_selection_actions()
        self.canvas.update()

    def update_selection_actions(self) -> None:
        selected = self.mode == MODE_SELECT and not self.selection.empty
        for action in (
            self.cut_action,
            self.copy_action,
            self.delete_action,
            self.duplicate_action,
            self.rotate_action,
            self.mirror_x_action,
            self.mirror_y_action,
        ):
            action.setEnabled(selected)
        self.paste_action.setEnabled(
            self.mode == MODE_SELECT and self.selection.anchor is not None and self.tile_clipboard is not None
        )
        self.deselect_action.setEnabled(True)
        self.tool_settings.sync_values()

    def copy_selection(self) -> None:
        self._edit_selection("Copy", copy_tiles=True, delete_tiles=False)

    def cut_selection(self) -> None:
        self._edit_selection("Cut", copy_tiles=True, delete_tiles=True)

    def delete_selection(self) -> None:
        if self.selection.area is None:
            refs = self.selection_refs()
            if refs:
                if self.apply_object_change("Delete Objects", selected_data(self.map_data, refs, remove=True)):
                    self.clear_selection()
            return
        self._edit_selection("Delete", copy_tiles=False, delete_tiles=True)

    def _edit_selection(self, operation: str, *, copy_tiles: bool, delete_tiles: bool) -> None:
        if self.selection.area is None:
            refs = self.selection_refs()
            if not refs:
                return
            try:
                block, region = copy_objects(self.map_data, refs, self.definitions)
            except ValueError as error:
                self.notify(str(error))
                return
            if delete_tiles and not self.apply_object_change(
                f"{operation} Objects", selected_data(self.map_data, refs, remove=True)
            ):
                return
            if delete_tiles:
                self.clear_selection()
            if copy_tiles:
                # An object block's level 0 is its lowest storey; the offset
                # keeps a paste relative to the viewed level, like a tile paste.
                payload = {"map": block, "objects": True, "view_offset": self.current_level - region.level}
                mime = QMimeData()
                mime.setData(CLIPBOARD_MIME, json.dumps(payload).encode("utf-8"))
                QApplication.clipboard().setMimeData(mime)
            return
        region = self.selection_region() if self.mode == MODE_SELECT else None
        if region is None:
            return
        try:
            block = copy_region(self.map_data, region) if copy_tiles else None
            after = delete_region(self.map_data, region) if delete_tiles else None
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
        if self.mode != MODE_SELECT or self.selection.anchor is None or self.tile_clipboard is None:
            return
        col, row = self.selection.anchor
        level = self.current_level - self.clipboard_view_offset if self.clipboard_objects else self.current_level
        try:
            block = number_checkpoint_copies(self.tile_clipboard, self.doc.root_data)
            after = (
                paste_objects(self.map_data, block, (col, row), level)
                if self.clipboard_objects
                else paste_region(self.map_data, block, (col, row), level)
            )
            added_errors = self.added_issues(after)
            if added_errors:
                raise ValueError("The pasted block conflicts with the destination:\n\n" + "\n".join(added_errors[:8]))
        except ValueError as exc:
            self.notify(f"Cannot paste: {exc}")
            return
        if not self.apply_change("Paste Objects" if self.clipboard_objects else "Paste Tiles", after):
            return
        if self.clipboard_objects:
            self.inspect_refs(refs_for_block(self.map_data, block, (col, row), level))
        else:
            self.set_tile_selection((col, row, col + block["grid_cols"], row + block["grid_rows"]))

    def apply_object_change(self, label, after):
        errors = self.added_issues(after)
        if errors:
            self.notify(errors[0])
            return False
        return self.apply_change(label, after)

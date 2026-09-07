"""Editing named geometry within one parent document."""

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .constants import DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS, MAP_NAME_RE
from .dialogs import ResizeMapDialog
from .io import empty_map


class NestedEditingMixin:
    def select_map(self, index: int) -> None:
        if index >= 0:
            self.doc.select_map(self.map_combo.itemData(index))

    def prompt_nested_name(self, title: str, current: str = "") -> str | None:
        name, accepted = QInputDialog.getText(self, title, "Name:", text=current)
        if not accepted:
            return None
        name = name.strip()
        if not MAP_NAME_RE.fullmatch(name):
            QMessageBox.warning(self, title, "Use only ASCII letters, digits, '_' or '-' in the name.")
            return None
        if name != current and name in self.doc.nested_geometry:
            QMessageBox.warning(self, title, f"Nested geometry {name!r} already exists.")
            return None
        return name

    def new_nested_map(self) -> None:
        name = self.prompt_nested_name("New Nested Map")
        if name is None:
            return
        dimensions = ResizeMapDialog.prompt(self, DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS)
        if dimensions is None:
            return
        geometry = empty_map(*dimensions[:2])
        geometry["player_spawn_zones"] = []
        after = copy.deepcopy(self.doc.root_data)
        after.setdefault("nested_geometry", {})[name] = geometry
        self.doc.apply_root_change("New Nested Map", after, name)

    def rename_nested_map(self) -> None:
        current = self.doc.active_map
        if current is None:
            return
        name = self.prompt_nested_name("Rename Nested Map", current)
        if name is None or name == current:
            return
        after = copy.deepcopy(self.doc.root_data)
        definitions = after["nested_geometry"]
        definitions[name] = definitions.pop(current)
        for geometry in [after, *definitions.values()]:
            for entry in geometry["nested_maps"]:
                if entry["map"] == current:
                    entry["map"] = name
        self.recent_nested_map = None
        self.doc.apply_root_change("Rename Nested Map", after, name)

    def delete_nested_map(self) -> None:
        name = self.doc.active_map
        if name is None:
            return
        users = [
            label for label, geometry in [("Outer map", self.doc.root_data), *self.doc.nested_geometry.items()]
            if label != name and any(entry["map"] == name for entry in geometry["nested_maps"])
        ]
        if users:
            QMessageBox.warning(self, "Nested Map In Use", "Remove its placements first from: " + ", ".join(users))
            return
        after = copy.deepcopy(self.doc.root_data)
        del after["nested_geometry"][name]
        self.doc.apply_root_change("Delete Nested Map", after, None)

"""Nested map placement actions for the editor window."""

from __future__ import annotations

import copy

from .constants import NESTED_MAPS_LIST
from .dialogs import MotionDialog
from .nesting import NestedMapShape, NestedMotion, nested_map_error, nested_map_shape


class NestedMapsMixin:
    # === Nested maps ===

    def nested_map_shape(self, name: str) -> NestedMapShape | None:
        return nested_map_shape(self.doc.nested_geometry.get(name))

    def nested_map_names(self) -> list[str]:
        return sorted(name for name in self.doc.nested_geometry if name != self.doc.active_map)

    def recent_nested_map_name(self) -> str | None:
        recent = self.recent_nested_map
        return recent.map_name if recent else None

    def edited_map_name(self) -> str | None:
        return self.doc.active_map

    def nested_map_end_at(self, cell: tuple[int, int]) -> tuple[dict, str] | None:
        """The nested map end anchored on `cell` on the current level, as
        `(entry, "from" | "to")`."""
        for entry in self.map_data.get(NESTED_MAPS_LIST, []):
            if entry["level"] == self.current_level and entry["from"] == list(cell):
                return entry, "from"
            if entry["to_level"] == self.current_level and entry["to"] == list(cell):
                return entry, "to"
        return None

    def add_nested_map(self, start_cell: tuple[int, int], end_cell: tuple[int, int]) -> None:
        hit = self.nested_map_end_at(start_cell)
        if hit is not None:
            where = "starts" if hit[1] == "from" else "ends"
            self.notify(f"A nested map already {where} here. Select it to edit or move it.")
            return
        recent = self.recent_nested_map
        if (
            recent is not None
            and 0 <= recent.to_level < len(self.map_data["levels"])
            and recent.map_name in self.nested_map_names()
        ):
            self.place_nested_map(start_cell, end_cell, recent)
            return
        result = MotionDialog.prompt_nested(
            self,
            len(self.map_data["levels"]),
            self.current_level,
            recent,
            self.nested_map_names(),
            self.switches,
        )
        if result is None:
            return
        self.recent_nested_map = result
        self.place_nested_map(start_cell, end_cell, result)

    def place_nested_map(self, start_cell: tuple[int, int], end_cell: tuple[int, int], motion: NestedMotion) -> None:
        msg = nested_map_error(motion.map_name, self.edited_map_name())
        if msg:
            self.notify(f"Nested map not placed: {msg}")
            return
        new_entry = {
            "level": self.current_level,
            "from": [start_cell[0], start_cell[1]],
            "to": [end_cell[0], end_cell[1]],
            **motion.to_entry(),
        }
        after = copy.deepcopy(self.map_data)
        # One nested map per starting cell and level: a new one replaces it.
        after[NESTED_MAPS_LIST] = [
            entry
            for entry in after.get(NESTED_MAPS_LIST, [])
            if not (entry["level"] == self.current_level and entry["from"] == new_entry["from"])
        ]
        after[NESTED_MAPS_LIST].append(new_entry)
        self.apply_change("Place Nested Map", after)

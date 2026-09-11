"""Nested map placement actions for the editor window."""

from __future__ import annotations

import copy

from .constants import NESTED_MAPS_LIST
from .dialogs import MotionDialog
from .nesting import NestedMapShape, NestedMotion, nested_map_error, nested_map_shape
from .normalization import nested_map_key


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
        `(entry, "from" | "to")`, so a drag that starts on an end moves it."""
        for entry in self.map_data.get(NESTED_MAPS_LIST, []):
            if entry["level"] == self.current_level and entry["from"] == list(cell):
                return entry, "from"
            if entry["to_level"] == self.current_level and entry["to"] == list(cell):
                return entry, "to"
        return None

    def drag_nested_map(self, start_cell: tuple[int, int], end_cell: tuple[int, int]) -> None:
        # A drag from an existing end moves that end; a click on an end
        # edits the entry; a click elsewhere places a map that stays put; any
        # other drag places one that slides.
        hit = self.nested_map_end_at(start_cell)
        if hit is None:
            self.add_nested_map(start_cell, end_cell)
            return
        entry, end = hit
        if end_cell == start_cell:
            self.edit_nested_map(nested_map_key(entry))
        else:
            self.move_nested_map_end(nested_map_key(entry), end, end_cell)

    def edit_nested_map(self, key: tuple) -> None:
        entry = next((e for e in self.map_data.get(NESTED_MAPS_LIST, []) if nested_map_key(e) == key), None)
        if entry is None:
            return
        result = MotionDialog.prompt_nested(
            self,
            len(self.map_data["levels"]),
            entry["level"],
            NestedMotion.from_entry(entry),
            self.nested_map_names(),
            self.switches,
            title="Edit Nested Map",
        )
        if result is None:
            return
        self.recent_nested_map = result
        self.set_nested_map_properties(key, result)

    def set_nested_map_properties(self, key: tuple, motion: NestedMotion) -> None:
        msg = nested_map_error(motion.map_name, self.edited_map_name())
        if msg:
            self.notify(f"Nested map not changed: {msg}")
            return
        after = copy.deepcopy(self.map_data)
        entry = next((e for e in after.get(NESTED_MAPS_LIST, []) if nested_map_key(e) == key), None)
        if entry is None:
            return
        entry.pop("switch", None)
        entry.pop("switch_inverted", None)
        entry.update(motion.to_entry())
        self.apply_change("Edit Nested Map", after)

    def move_nested_map_end(self, key: tuple, end: str, cell: tuple[int, int]) -> None:
        after = copy.deepcopy(self.map_data)
        entries = after.get(NESTED_MAPS_LIST, [])
        entry = next((e for e in entries if nested_map_key(e) == key), None)
        if entry is None:
            return
        moved = [cell[0], cell[1]]
        if end == "from" and any(
            e is not entry and e["level"] == entry["level"] and e["from"] == moved for e in entries
        ):
            self.notify("Nested map end not moved: another nested map starts on that cell")
            return
        entry[end] = moved
        self.apply_change("Move Nested Map End", after)

    def add_nested_map(self, start_cell: tuple[int, int], end_cell: tuple[int, int]) -> None:
        recent = self.recent_nested_map
        if (recent is not None
                and 0 <= recent.to_level < len(self.map_data["levels"])
                and recent.map_name in self.nested_map_names()):
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

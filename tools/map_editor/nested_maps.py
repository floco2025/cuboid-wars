"""Nested map placement actions for the editor window."""

from __future__ import annotations

import copy
from dataclasses import dataclass

from .constants import MAPS_DIR, NESTED_MAPS_LIST, list_map_names
from .dialogs import MotionDialog, Nudge
from .io import read_map
from .normalization import nested_map_key


@dataclass(frozen=True)
class NestedMapShape:
    """What the canvas and the validator need to know about a nested map's
    file: its footprint, its storey count, and the maps it nests itself."""

    grid_cols: int
    grid_rows: int
    level_count: int
    nested_names: tuple[str, ...]


def load_nested_map_shape(name: str) -> NestedMapShape | None:
    """The shape of `config/server/maps/<name>.json`, or None when the file
    is missing or unreadable."""
    path = MAPS_DIR / f"{name}.json"
    if not path.is_file():
        return None
    try:
        data = read_map(path)
    except Exception:
        return None
    return NestedMapShape(
        grid_cols=data["grid_cols"],
        grid_rows=data["grid_rows"],
        level_count=len(data["levels"]),
        nested_names=tuple(entry["map"] for entry in data.get(NESTED_MAPS_LIST, [])),
    )


def nested_map_cycle(edited: str | None, entries: list[dict], lookup) -> list[str] | None:
    """The chain of names along which a map nests itself, starting from the
    edited map's entries, or None. `lookup(name)` gives a map's shape (None
    for an unknown file, which ends that branch)."""
    checked: set[str] = set()

    def visit(name: str, chain: list[str]) -> list[str] | None:
        if name in chain:
            return chain[chain.index(name) :] + [name]
        if name in checked:
            return None
        shape = lookup(name)
        if shape is None:
            return None
        for child in shape.nested_names:
            found = visit(child, chain + [name])
            if found:
                return found
        checked.add(name)
        return None

    root = edited or "(this map)"
    for entry in entries:
        found = visit(entry["map"], [root])
        if found:
            return found
    return None


def nested_map_rest_points(entry: dict, wall_width_cells: float) -> tuple[tuple[float, float], tuple[float, float]]:
    """Where the nested map's cell (0, 0) rests at each end, in cell units:
    the anchor plus the nudge's x and z parts. The y part is left out; the
    canvas shows the plan, and a storey's height is not a canvas distance."""

    def rest(anchor: list[int], nudge: list[float]) -> tuple[float, float]:
        if len(nudge) != 3:
            return tuple(anchor)
        return (anchor[0] + nudge[0] * wall_width_cells, anchor[1] + nudge[2] * wall_width_cells)

    return rest(entry["from"], entry["from_nudge"]), rest(entry["to"], entry["to_nudge"])


def nested_map_label(name: str, nudge: list[float]) -> str:
    """The footprint's label: the map's name, and its y nudge when it has
    one, since the plan cannot draw a vertical displacement."""
    return name if len(nudge) != 3 or nudge[1] == 0 else f"{name} y{nudge[1]:+g}"


def nested_map_error(map_name: str, edited: str | None) -> str | None:
    if not map_name:
        return "pick a map to nest"
    if edited is not None and map_name == edited:
        return "a map cannot nest itself"
    return None


class NestedMapsMixin:
    # === Nested maps ===

    def nested_map_shape(self, name: str) -> NestedMapShape | None:
        """Memoised per name: hotel.json is large, and the validator asks on
        every change."""
        shapes = self.__dict__.setdefault("nested_map_shapes", {})
        if name not in shapes:
            shapes[name] = load_nested_map_shape(name)
        return shapes[name]

    def forget_nested_map_shapes(self) -> None:
        self.nested_map_shapes = {}

    def recent_nested_map_name(self) -> str | None:
        recent = getattr(self, "recent_nested_map", None)
        return recent[0] if recent else None

    def edited_map_name(self) -> str | None:
        path = getattr(self, "path", None)
        return path.stem if path is not None else None

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
        current = (
            entry["map"],
            entry["to_level"],
            entry["travel_secs"],
            entry["pause_secs"],
            entry["phase_secs"],
            tuple(entry["from_nudge"]),
            tuple(entry["to_nudge"]),
        )
        result = MotionDialog.prompt_nested(
            self,
            len(self.map_data["levels"]),
            entry["level"],
            current,
            list_map_names(exclude=self.edited_map_name()),
            title="Edit Nested Map",
        )
        if result is None:
            return
        self.recent_nested_map = result
        self.set_nested_map_properties(key, *result)

    def set_nested_map_properties(
        self,
        key: tuple,
        map_name: str,
        to_level: int,
        travel_secs: float,
        pause_secs: float,
        phase_secs: float,
        from_nudge: Nudge,
        to_nudge: Nudge,
    ) -> None:
        msg = nested_map_error(map_name, self.edited_map_name())
        if msg:
            self.notify(f"Nested map not changed: {msg}")
            return
        after = copy.deepcopy(self.map_data)
        entry = next((e for e in after.get(NESTED_MAPS_LIST, []) if nested_map_key(e) == key), None)
        if entry is None:
            return
        entry.update(
            {
                "map": map_name,
                "to_level": to_level,
                "travel_secs": travel_secs,
                "pause_secs": pause_secs,
                "phase_secs": phase_secs,
                "from_nudge": list(from_nudge),
                "to_nudge": list(to_nudge),
            }
        )
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
        if (self.recent_nested_map is not None
                and 0 <= self.recent_nested_map[1] < len(self.map_data["levels"])
                and self.recent_nested_map[0] in list_map_names(exclude=self.edited_map_name())):
            self.place_nested_map(start_cell, end_cell, *self.recent_nested_map)
            return
        result = MotionDialog.prompt_nested(
            self,
            len(self.map_data["levels"]),
            self.current_level,
            self.recent_nested_map,
            list_map_names(exclude=self.edited_map_name()),
        )
        if result is None:
            return
        self.recent_nested_map = result
        self.place_nested_map(start_cell, end_cell, *result)

    def place_nested_map(
        self,
        start_cell: tuple[int, int],
        end_cell: tuple[int, int],
        map_name: str,
        to_level: int,
        travel_secs: float,
        pause_secs: float,
        phase_secs: float,
        from_nudge: Nudge,
        to_nudge: Nudge,
    ) -> None:
        msg = nested_map_error(map_name, self.edited_map_name())
        if msg:
            self.notify(f"Nested map not placed: {msg}")
            return
        new_entry = {
            "map": map_name,
            "level": self.current_level,
            "from": [start_cell[0], start_cell[1]],
            "to": [end_cell[0], end_cell[1]],
            "to_level": to_level,
            "travel_secs": travel_secs,
            "pause_secs": pause_secs,
            "phase_secs": phase_secs,
            "from_nudge": list(from_nudge),
            "to_nudge": list(to_nudge),
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

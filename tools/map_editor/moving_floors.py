"""Moving floor placement actions for the editor window."""

from __future__ import annotations

import copy

from .constants import MOVING_FLOORS_LIST
from .dialogs import MovingFloorDialog
from .erase import moving_floors_outside
from .geometry import rect_from_cells
from .normalization import moving_floor_key


def moving_floor_error(start_cell: tuple[int, int], end_cell: tuple[int, int], level: int, to_level: int) -> str | None:
    if start_cell == end_cell and level == to_level:
        return "a moving floor must travel; drag to another cell or pick another level"
    return None


class MovingFloorsMixin:
    # === Moving floors ===

    def moving_floor_end_at(self, cell: tuple[int, int]) -> tuple[dict, str] | None:
        """The tile end sitting on `cell` on the current level, as
        `(floor, "from" | "to")`, so a drag that starts on an end moves it."""
        for floor in self.map_data.get(MOVING_FLOORS_LIST, []):
            if floor["level"] == self.current_level and floor["from"] == list(cell):
                return floor, "from"
            if floor["to_level"] == self.current_level and floor["to"] == list(cell):
                return floor, "to"
        return None

    def drag_moving_floor(self, start_cell: tuple[int, int], end_cell: tuple[int, int]) -> None:
        # A drag from an existing end moves that end (its storey stays), so
        # a lift's far end can be placed from the storey it lands on; a click
        # on an end edits the tile's properties; any other drag lays a new
        # tile.
        hit = self.moving_floor_end_at(start_cell)
        if hit is None:
            self.add_moving_floor(start_cell, end_cell)
            return
        floor, end = hit
        if end_cell == start_cell:
            self.edit_moving_floor(moving_floor_key(floor))
        else:
            self.move_moving_floor_end(moving_floor_key(floor), end, end_cell)

    def edit_moving_floor(self, key: tuple) -> None:
        floor = next((f for f in self.map_data.get(MOVING_FLOORS_LIST, []) if moving_floor_key(f) == key), None)
        if floor is None:
            return
        current = (floor["to_level"], floor["speed"], floor["pause_secs"], floor["phase_secs"])
        result = MovingFloorDialog.prompt(
            self, len(self.map_data["levels"]), floor["level"], current, title="Edit Moving Floor"
        )
        if result is None:
            return
        self.recent_moving_floor = result
        self.set_moving_floor_properties(key, *result)

    def set_moving_floor_properties(
        self, key: tuple, to_level: int, speed: float, pause_secs: float, phase_secs: float
    ) -> None:
        after = copy.deepcopy(self.map_data)
        floor = next((f for f in after.get(MOVING_FLOORS_LIST, []) if moving_floor_key(f) == key), None)
        if floor is None:
            return
        msg = moving_floor_error(tuple(floor["from"]), tuple(floor["to"]), floor["level"], to_level)
        if msg:
            self._flash_status(f"Moving floor not changed: {msg}")
            return
        floor.update({"to_level": to_level, "speed": speed, "pause_secs": pause_secs, "phase_secs": phase_secs})
        self.apply_change("Edit Moving Floor", after)

    def move_moving_floor_end(self, key: tuple, end: str, cell: tuple[int, int]) -> None:
        after = copy.deepcopy(self.map_data)
        floors = after.get(MOVING_FLOORS_LIST, [])
        floor = next((f for f in floors if moving_floor_key(f) == key), None)
        if floor is None:
            return
        moved = {**floor, end: [cell[0], cell[1]]}
        msg = moving_floor_error(tuple(moved["from"]), tuple(moved["to"]), moved["level"], moved["to_level"])
        if msg:
            self._flash_status(f"Moving floor end not moved: {msg}")
            return
        if end == "from" and any(
            f is not floor and f["level"] == floor["level"] and f["from"] == moved["from"] for f in floors
        ):
            self._flash_status("Moving floor end not moved: another moving floor starts on that cell")
            return
        floor[end] = moved[end]
        self.apply_change("Move Moving Floor End", after)

    def add_moving_floor(self, start_cell: tuple[int, int], end_cell: tuple[int, int]) -> None:
        result = MovingFloorDialog.prompt(
            self, len(self.map_data["levels"]), self.current_level, self.recent_moving_floor
        )
        if result is None:
            return
        self.recent_moving_floor = result
        to_level, speed, pause_secs, phase_secs = result
        self.place_moving_floor(start_cell, end_cell, to_level, speed, pause_secs, phase_secs)

    def place_moving_floor(
        self,
        start_cell: tuple[int, int],
        end_cell: tuple[int, int],
        to_level: int,
        speed: float,
        pause_secs: float,
        phase_secs: float,
    ) -> None:
        msg = moving_floor_error(start_cell, end_cell, self.current_level, to_level)
        if msg:
            self._flash_status(f"Moving floor not placed: {msg}")
            return
        new_floor = {
            "level": self.current_level,
            "from": [start_cell[0], start_cell[1]],
            "to": [end_cell[0], end_cell[1]],
            "to_level": to_level,
            "speed": speed,
            "pause_secs": pause_secs,
            "phase_secs": phase_secs,
            **self._face_materials_for_current(),
        }
        after = copy.deepcopy(self.map_data)
        # One tile per starting cell and level: a new one replaces it.
        after[MOVING_FLOORS_LIST] = [
            floor
            for floor in after.get(MOVING_FLOORS_LIST, [])
            if not (floor["level"] == self.current_level and floor["from"] == new_floor["from"])
        ]
        after[MOVING_FLOORS_LIST].append(new_floor)
        self.apply_change("Place Moving Floor", after)

    def erase_moving_floors_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        floors = self.map_data.get(MOVING_FLOORS_LIST, [])
        kept = moving_floors_outside(floors, self.current_level, rect)
        if len(kept) == len(floors):
            self._flash_status("Erase Moving Floors: no moving floors in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after[MOVING_FLOORS_LIST] = kept
        self.apply_change("Erase Moving Floors", after)

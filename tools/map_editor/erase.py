"""Erase actions and feedback for the editor window."""

from __future__ import annotations

from .constants import FLOOR_HIT_KINDS, MODE_ERASE_SPAWN_ZONES
from .geometry import rect_from_cells
from .types import ZoneRef
from .erasing import ERASE_GROUPS, erase_cell_rect, erase_group_rect, erase_hit, hit_at


class EraseMixin:
    def erase_at(self, pos, preserve_floors: bool) -> None:
        hit = self.hit_at(pos)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(label, erase_cell_rect(self.map_data, self.current_level, start, end, preserve_floors))

    # The `Erase <group>` tools: clear one record group inside the dragged
    # rectangle on the current level.
    def erase_group_rect(self, mode: str, start: tuple[int, int], end: tuple[int, int]) -> None:
        noun, _ = ERASE_GROUPS[mode]
        after = erase_group_rect(self.map_data, mode, self.current_level, rect_from_cells(start, end))
        if after is None:
            self.notify(f"{mode}: no {noun} in selection.")
            return
        if mode == MODE_ERASE_SPAWN_ZONES:
            self.selected_spawn_zone_ref = None
        self.apply_change(mode, after)

    def hit_at(self, pos):
        return hit_at(self.map_data, self.current_level, pos.x(), pos.y())

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        if hit[0] == "Spawn Zone" and self.selected_spawn_zone_ref == ZoneRef(*hit[1]):
            self.selected_spawn_zone_ref = None
        self.apply_change(f"Erase {hit[0]}", erase_hit(self.map_data, self.current_level, hit, preserve_floors))

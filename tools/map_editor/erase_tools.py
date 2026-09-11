"""Erase actions and feedback for the editor window."""

from __future__ import annotations

from . import erasing
from .constants import FLOOR_HIT_KINDS, HIT_SPAWN_ZONE, HIT_CHECKPOINT, MODE_ERASE_CHECKPOINTS, MODE_ERASE_SPAWN_ZONES
from .geometry import rect_from_cells
from .types import ZoneRef


class EraseMixin:
    def erase_at(self, pos, preserve_floors: bool) -> None:
        hit = self.hit_at(pos)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(
            label, erasing.erase_cell_rect(self.map_data, self.current_level, start, end, preserve_floors)
        )

    # The `Erase <group>` tools: clear one record group inside the dragged
    # rectangle on the current level.
    def erase_group_rect(self, mode: str, start: tuple[int, int], end: tuple[int, int]) -> None:
        noun, _ = erasing.ERASE_GROUPS[mode]
        after = erasing.erase_group_rect(self.map_data, mode, self.current_level, rect_from_cells(start, end))
        if after is None:
            self.notify(f"{mode}: no {noun} in selection.")
            return
        if mode in (MODE_ERASE_SPAWN_ZONES, MODE_ERASE_CHECKPOINTS):
            self.selected_spawn_zone_ref = None
        self.apply_change(mode, after)

    def hit_at(self, pos):
        return erasing.hit_at(self.map_data, self.current_level, pos.x(), pos.y(), self.canvas.pick_tolerance())

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        if hit[0] in (HIT_SPAWN_ZONE, HIT_CHECKPOINT) and self.selected_spawn_zone_ref == ZoneRef(*hit[1]):
            self.selected_spawn_zone_ref = None
        self.apply_change(f"Erase {hit[0]}", erasing.erase_hit(self.map_data, self.current_level, hit, preserve_floors))

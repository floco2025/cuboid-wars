"""Erase actions and feedback for the editor window."""

from __future__ import annotations

import copy

from .constants import FLOOR_HIT_KINDS, SPAWN_ZONE_LISTS
from .geometry import rect_from_cells
from .types import ZoneRef
from .erasing import (
    cells_outside, edges_outside, erase_cell_rect, erase_hit, hit_at,
    ramps_outside, zones_outside,
)


class EraseMixin:
    # === Erase / hit-testing ===

    def erase_at(self, pos, cell_size: float, preserve_floors: bool) -> None:
        hit = self.hit_at(pos, cell_size)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(label, erase_cell_rect(self.map_data, self.current_level, start, end, preserve_floors))

    def erase_floors_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        level = self.map_data["levels"][self.current_level]
        floors = cells_outside(level["floors"], rect)
        inaccessible = cells_outside(level["inaccessible_floors"], rect)
        if len(floors) == len(level["floors"]) and len(inaccessible) == len(level["inaccessible_floors"]):
            self._flash_status("Erase Floors: no floors in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][self.current_level]["floors"] = floors
        after["levels"][self.current_level]["inaccessible_floors"] = inaccessible
        self.apply_change("Erase Floors", after)

    def erase_walls_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self._erase_level_edges("walls", "Erase Walls", start, end)

    def erase_barriers_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self._erase_level_edges("barriers", "Erase Barriers", start, end)

    def _erase_level_edges(self, list_name: str, label: str, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        edges = self.map_data["levels"][self.current_level].get(list_name, [])
        kept = edges_outside(edges, rect)
        if len(kept) == len(edges):
            self._flash_status(f"{label}: no {list_name} in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][self.current_level][list_name] = kept
        self.apply_change(label, after)

    def erase_ramps_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        kept = ramps_outside(self.map_data["ramps"], self.current_level, rect)
        if len(kept) == len(self.map_data["ramps"]):
            self._flash_status("Erase Ramps: no ramps in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after["ramps"] = kept
        self.apply_change("Erase Ramps", after)

    def erase_spawn_zones_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        kept = {name: zones_outside(self.map_data[name], self.current_level, rect) for name in SPAWN_ZONE_LISTS}
        if all(len(kept[name]) == len(self.map_data[name]) for name in SPAWN_ZONE_LISTS):
            self._flash_status("Erase Spawn Zones: no spawn zones in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after.update(kept)
        self.selected_spawn_zone_ref = None
        self.apply_change("Erase Spawn Zones", after)

    def hit_at(self, pos, cell_size: float):
        return hit_at(self.map_data, self.current_level, pos, cell_size)

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        if hit[0] == "Spawn Zone" and self.selected_spawn_zone_ref == ZoneRef(*hit[1]):
            self.selected_spawn_zone_ref = None
        self.apply_change(f"Erase {hit[0]}", erase_hit(self.map_data, self.current_level, hit, preserve_floors))

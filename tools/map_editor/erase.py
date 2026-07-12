"""Erase and hit-testing actions for the editor window."""

from __future__ import annotations

import copy

from .constants import FLOOR_HIT_KINDS, MODE_FLOOR, MODE_GRASS, MODE_INACCESSIBLE_FLOOR, SPAWN_ZONE_LISTS
from .geometry import (
    normalized_wall,
    point_near_wall,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
    wall_overlaps_rect,
    zone_contains_cell,
    zone_intersects_rect,
)
from .types import ZoneRef


class EraseMixin:
    # === Erase / hit-testing ===

    def erase_at(self, pos, cell_size: float, preserve_floors: bool) -> None:
        hit = self.hit_at(pos, cell_size)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if not preserve_floors:
            level["floors"] = [
                floor
                for floor in level["floors"]
                if not (c0 <= floor["col"] < c1 and r0 <= floor["row"] < r1)
            ]
            level["inaccessible_floors"] = [
                floor
                for floor in level["inaccessible_floors"]
                if not (c0 <= floor["col"] < c1 and r0 <= floor["row"] < r1)
            ]
        # Grass is decoration, not a floor — both erase modes remove it.
        level["grass"] = [
            grass
            for grass in level.get("grass", [])
            if not (c0 <= grass["col"] < c1 and r0 <= grass["row"] < r1)
        ]
        level["walls"] = [
            wall
            for wall in level["walls"]
            if not wall_overlaps_rect([wall["c0"], wall["r0"], wall["c1"], wall["r1"]], (c0, r0, c1, r1))
        ]
        level["barriers"] = [
            barrier
            for barrier in level.get("barriers", [])
            if not wall_overlaps_rect([barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]], (c0, r0, c1, r1))
        ]
        for list_name in SPAWN_ZONE_LISTS:
            after[list_name] = [
                zone
                for zone in after[list_name]
                if not (zone["level"] == self.current_level and zone_intersects_rect(zone, (c0, r0, c1, r1)))
            ]
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if self.current_level not in (ramp["lower_level"], ramp["lower_level"] + 1)
            or not rects_overlap((c0, r0, c1, r1), ramp_rect(ramp))
        ]
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(label, after)

    def hit_at(self, pos, cell_size: float):
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        level = self.map_data["levels"][self.current_level]
        px = pos.x() / cell_size
        py = pos.y() / cell_size

        for wall in level["walls"]:
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr):
                return ("Wall", tuple(wall_arr))
        for barrier in level.get("barriers", []):
            arr = [barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]]
            if point_near_wall(px, py, arr):
                return ("Barrier", tuple(arr))
        # Walk every zone list in reverse so the most-recently-painted entry
        # wins. SPAWN_ZONE_LISTS is ordered actor → player, so when both zone
        # types share a cell the actor zone is preferred.
        for list_name in SPAWN_ZONE_LISTS:
            for idx in range(len(self.map_data[list_name]) - 1, -1, -1):
                zone = self.map_data[list_name][idx]
                if zone["level"] == self.current_level and zone_contains_cell(zone, col, row):
                    return ("Spawn Zone", (list_name, idx))
        for ramp in self.map_data["ramps"]:
            lower = ramp["lower_level"]
            if self.current_level not in (lower, lower + 1):
                continue
            c0, r0, c1, r1 = ramp_rect(ramp)
            if c0 <= col < c1 and r0 <= row < r1:
                return ("Ramp", (lower, tuple(ramp["low"]), tuple(ramp["high"])))
        # Grass sits on top of a floor, so a click peels the grass first; the
        # next click then hits the floor underneath.
        if any(g["col"] == col and g["row"] == row for g in level.get("grass", [])):
            return (MODE_GRASS, (col, row))
        if any(f["col"] == col and f["row"] == row for f in level["floors"]):
            return (MODE_FLOOR, (col, row))
        if any(f["col"] == col and f["row"] == row for f in level["inaccessible_floors"]):
            return (MODE_INACCESSIBLE_FLOOR, (col, row))
        return None

    def erase_hit(self, hit, preserve_floors: bool = False) -> None:
        kind, value = hit
        if preserve_floors and kind in FLOOR_HIT_KINDS:
            return
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        if kind == MODE_GRASS:
            level["grass"] = [
                grass for grass in level.get("grass", []) if (grass["col"], grass["row"]) != value
            ]
        elif kind == MODE_FLOOR:
            level["floors"] = [
                floor for floor in level["floors"] if (floor["col"], floor["row"]) != value
            ]
        elif kind == MODE_INACCESSIBLE_FLOOR:
            level["inaccessible_floors"] = [
                floor for floor in level["inaccessible_floors"]
                if (floor["col"], floor["row"]) != value
            ]
        elif kind == "Spawn Zone":
            list_name, target_idx = value
            if 0 <= target_idx < len(after[list_name]):
                del after[list_name][target_idx]
                if self.selected_spawn_zone_ref == ZoneRef(list_name, target_idx):
                    self.selected_spawn_zone_ref = None
        elif kind == "Wall":
            level["walls"] = [
                wall for wall in level["walls"]
                if tuple(normalized_wall([wall["c0"], wall["r0"], wall["c1"], wall["r1"]])) != value
            ]
        elif kind == "Barrier":
            level["barriers"] = [
                barrier for barrier in level.get("barriers", [])
                if tuple(normalized_wall([barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]])) != value
            ]
        elif kind == "Ramp":
            lower, low, high = value
            after["ramps"] = [
                ramp
                for ramp in after["ramps"]
                if (ramp["lower_level"], tuple(ramp["low"]), tuple(ramp["high"])) != (lower, low, high)
            ]
        self.apply_change(f"Erase {kind}", after)

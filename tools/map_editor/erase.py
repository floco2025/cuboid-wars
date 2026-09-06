"""Erase and hit-testing actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    FLOOR_HIT_KINDS,
    ITEMS_LIST,
    MODE_FLOOR,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    NESTED_MAPS_LIST,
    SPAWN_ZONE_LISTS,
)
from .geometry import (
    cell_side_from_click,
    normalized_wall,
    point_near_wall,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
    wall_endpoints_for_cell_side,
    wall_overlaps_rect,
    zone_contains_cell,
    zone_intersects_rect,
)
from .normalization import ladder_key, ladder_spans_level, nested_map_key
from .types import ZoneRef

Rect = tuple[int, int, int, int]


def cells_outside(entries: list[dict], rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect
    return [e for e in entries if not (c0 <= e["col"] < c1 and r0 <= e["row"] < r1)]


def level_cells_outside(entries: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect
    return [
        e for e in entries if not (e["level"] == level_idx and c0 <= e["col"] < c1 and r0 <= e["row"] < r1)
    ]


def edges_outside(entries: list[dict], rect: Rect) -> list[dict]:
    return [e for e in entries if not wall_overlaps_rect([e["c0"], e["r0"], e["c1"], e["r1"]], rect)]


def ramps_outside(ramps: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    return [
        ramp
        for ramp in ramps
        if level_idx not in (ramp["lower_level"], ramp["lower_level"] + 1) or not rects_overlap(rect, ramp_rect(ramp))
    ]


def zones_outside(zones: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    return [zone for zone in zones if not (zone["level"] == level_idx and zone_intersects_rect(zone, rect))]


def ladders_outside(ladders: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    return [
        ladder
        for ladder in ladders
        if not ladder_spans_level(ladder, level_idx)
        or not wall_overlaps_rect(
            list(wall_endpoints_for_cell_side(ladder["col"], ladder["row"], ladder["side"])), rect
        )
    ]


def nested_maps_outside(entries: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect

    def end_in_rect(cell: list[int], level: int) -> bool:
        return level == level_idx and c0 <= cell[0] < c1 and r0 <= cell[1] < r1

    return [
        entry
        for entry in entries
        if not (end_in_rect(entry["from"], entry["level"]) or end_in_rect(entry["to"], entry["to_level"]))
    ]


class EraseMixin:
    # === Erase / hit-testing ===

    def erase_at(self, pos, cell_size: float, preserve_floors: bool) -> None:
        hit = self.hit_at(pos, cell_size)
        if hit and not (preserve_floors and hit[0] in FLOOR_HIT_KINDS):
            self.erase_hit(hit, preserve_floors)

    def erase_cell_rect(self, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool) -> None:
        rect = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level_idx = self.current_level
        level = after["levels"][level_idx]
        # Keep Floors keeps the slabs and what stands on them; everything
        # else in the rectangle goes in both modes. Lights need no line of
        # their own: canonicalization drops them with their wall.
        if not preserve_floors:
            level["floors"] = cells_outside(level["floors"], rect)
            level["inaccessible_floors"] = cells_outside(level["inaccessible_floors"], rect)
            level["light_bridges"] = cells_outside(level.get("light_bridges", []), rect)
            after[ITEMS_LIST] = level_cells_outside(after.get(ITEMS_LIST, []), level_idx, rect)
            after["pressure_plates"] = level_cells_outside(after.get("pressure_plates", []), level_idx, rect)
            after[NESTED_MAPS_LIST] = nested_maps_outside(after.get(NESTED_MAPS_LIST, []), level_idx, rect)
        level["grass"] = cells_outside(level.get("grass", []), rect)
        level["walls"] = edges_outside(level["walls"], rect)
        level["barriers"] = edges_outside(level.get("barriers", []), rect)
        for list_name in SPAWN_ZONE_LISTS:
            after[list_name] = zones_outside(after[list_name], level_idx, rect)
        after["ramps"] = ramps_outside(after["ramps"], level_idx, rect)
        after["ladders"] = ladders_outside(after.get("ladders", []), level_idx, rect)
        label = "Erase Non-Floor Area" if preserve_floors else "Erase Area"
        self.apply_change(label, after)

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
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        level = self.map_data["levels"][self.current_level]
        px = pos.x() / cell_size
        py = pos.y() / cell_size

        # A light sits on the wall side nearest the click, as when placing
        # one, and peels before the wall it hangs on.
        side = cell_side_from_click(col, row, px, py)
        if any((l["col"], l["row"], l["side"]) == (col, row, side) for l in level.get("lights", [])):
            return ("Light", (col, row, side))
        for wall in level["walls"]:
            wall_arr = [wall["c0"], wall["r0"], wall["c1"], wall["r1"]]
            if point_near_wall(px, py, wall_arr):
                return ("Wall", tuple(wall_arr))
        for barrier in level.get("barriers", []):
            arr = [barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]]
            if point_near_wall(px, py, arr):
                return ("Barrier", tuple(arr))
        for ladder in self.map_data.get("ladders", []):
            if not ladder_spans_level(ladder, self.current_level):
                continue
            edge = list(wall_endpoints_for_cell_side(ladder["col"], ladder["row"], ladder["side"]))
            if point_near_wall(px, py, edge):
                return ("Ladder", ladder_key(ladder))
        on_level = self.current_level
        if any(p["level"] == on_level and (p["col"], p["row"]) == (col, row) for p in self.map_data.get("pressure_plates", [])):
            return ("Pressure Plate", (col, row))
        if any(i["level"] == on_level and (i["col"], i["row"]) == (col, row) for i in self.map_data.get(ITEMS_LIST, [])):
            return ("Item", (col, row))
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
        if any(b["col"] == col and b["row"] == row for b in level.get("light_bridges", [])):
            return (MODE_LIGHT_BRIDGE, (col, row))
        # Only a nested map's anchor cells are hit targets: whatever lies
        # under its footprint stays clickable.
        for entry in self.map_data.get(NESTED_MAPS_LIST, []):
            at_start = entry["level"] == self.current_level and entry["from"] == [col, row]
            at_end = entry["to_level"] == self.current_level and entry["to"] == [col, row]
            if at_start or at_end:
                return (MODE_NESTED_MAP, nested_map_key(entry))
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
        elif kind == MODE_LIGHT_BRIDGE:
            level["light_bridges"] = [
                bridge for bridge in level.get("light_bridges", []) if (bridge["col"], bridge["row"]) != value
            ]
        elif kind == MODE_NESTED_MAP:
            after[NESTED_MAPS_LIST] = [
                entry for entry in after.get(NESTED_MAPS_LIST, []) if nested_map_key(entry) != value
            ]
        elif kind == "Spawn Zone":
            list_name, target_idx = value
            if 0 <= target_idx < len(after[list_name]):
                del after[list_name][target_idx]
                if self.selected_spawn_zone_ref == ZoneRef(list_name, target_idx):
                    self.selected_spawn_zone_ref = None
        elif kind == "Light":
            level["lights"] = [
                light for light in level.get("lights", []) if (light["col"], light["row"], light["side"]) != value
            ]
        elif kind == "Pressure Plate":
            after["pressure_plates"] = [
                plate
                for plate in after.get("pressure_plates", [])
                if not (plate["level"] == self.current_level and (plate["col"], plate["row"]) == value)
            ]
        elif kind == "Item":
            after[ITEMS_LIST] = [
                item
                for item in after.get(ITEMS_LIST, [])
                if not (item["level"] == self.current_level and (item["col"], item["row"]) == value)
            ]
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
        elif kind == "Ladder":
            after["ladders"] = [
                ladder for ladder in after.get("ladders", []) if ladder_key(ladder) != value
            ]
        self.apply_change(f"Erase {kind}", after)

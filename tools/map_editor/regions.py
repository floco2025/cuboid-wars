"""Tile volumes and clipboard operations, independent of widgets."""

from __future__ import annotations

import copy
from dataclasses import dataclass

from .constants import LIGHT_SIDES, SPAWN_ZONE_LISTS
from .geometry import rects_overlap, wall_endpoints_for_cell_side, wall_overlaps_rect
from .normalization import edge_key
from .io import empty_level, empty_map
from .transforms import EDGE_LISTS, GLOBAL_LISTS, LEVEL_LISTS, record_levels, record_rect, translate_map


@dataclass(frozen=True)
class TileRegion:
    rect: tuple[int, int, int, int]
    level: int
    levels: int = 1

    @property
    def top(self) -> int:
        return self.level + self.levels

    def contains_cell(self, col: int, row: int) -> bool:
        c0, r0, c1, r1 = self.rect
        return c0 <= col < c1 and r0 <= row < r1

    def contains_rect(self, rect: tuple[int, int, int, int]) -> bool:
        c0, r0, c1, r1 = self.rect
        return c0 <= rect[0] and r0 <= rect[1] and rect[2] <= c1 and rect[3] <= r1

    def contains_level(self, level: int) -> bool:
        return self.level <= level < self.top

    def check_bounds(self, data: dict) -> None:
        c0, r0, c1, r1 = self.rect
        if not (0 <= c0 < c1 <= data["grid_cols"] and 0 <= r0 < r1 <= data["grid_rows"]):
            raise ValueError("The block does not fit inside the map. Choose another tile or resize the map.")
        if self.level < 0 or self.levels < 1 or self.top > len(data["levels"]):
            raise ValueError("The selected levels are outside the map.")


def _edge(entry: dict) -> list[int]:
    return [entry[key] for key in ("c0", "r0", "c1", "r1")]


def _whole_object(region: TileRegion, rect: tuple[int, int, int, int], lower: int, upper: int, name: str) -> bool:
    touches = lower < region.top and region.level <= upper and rects_overlap(region.rect, rect)
    if touches and not (region.contains_rect(rect) and region.level <= lower and upper < region.top):
        raise ValueError(f"The selection crosses a {name}. Include its whole footprint and all its levels.")
    return touches


# The whole-object rule for every record spanning cells or levels; a
# nested map's two ends are judged separately.
WHOLE_OBJECT_NOUNS = {**dict.fromkeys(SPAWN_ZONE_LISTS, "spawn zone"), "ramps": "ramp", "ladders": "ladder"}


def _global_selected(name: str, entry: dict, region: TileRegion) -> bool:
    if name in WHOLE_OBJECT_NOUNS:
        lower, upper = record_levels(entry)
        return _whole_object(region, record_rect(name, entry), lower, upper, WHOLE_OBJECT_NOUNS[name])
    if name == "nested_maps":
        start = region.contains_level(entry["level"]) and region.contains_cell(*entry["from"])
        end = region.contains_level(entry["to_level"]) and region.contains_cell(*entry["to"])
        if start != end:
            raise ValueError("The selection crosses a nested map's motion. Include both end tiles and their levels.")
        return start
    return region.contains_level(entry["level"]) and region.contains_cell(entry["col"], entry["row"])


def _partition(data: dict, region: TileRegion) -> tuple[dict, dict]:
    region.check_bounds(data)
    chosen = empty_map(data["grid_cols"], data["grid_rows"])
    chosen["levels"] = []
    remaining = copy.deepcopy(data)
    for index in range(region.level, region.top):
        source = data["levels"][index]
        selected = {"name": source["name"]}
        for name in LEVEL_LISTS:
            selected[name] = []
            remaining["levels"][index][name] = []
            for entry in source.get(name, []):
                inside = (
                    wall_overlaps_rect(_edge(entry), region.rect) if name in EDGE_LISTS
                    else region.contains_cell(entry["col"], entry["row"])
                )
                target = selected[name] if inside else remaining["levels"][index][name]
                target.append(copy.deepcopy(entry))
        chosen["levels"].append(selected)
    for name in GLOBAL_LISTS:
        chosen[name] = []
        remaining[name] = []
        for entry in data.get(name, []):
            target = chosen[name] if _global_selected(name, entry, region) else remaining[name]
            target.append(copy.deepcopy(entry))
    return chosen, remaining


def copy_region(data: dict, region: TileRegion) -> dict:
    chosen, _ = _partition(data, region)
    c0, r0, c1, r1 = region.rect
    chosen = translate_map(chosen, -c0, -r0, -region.level)
    chosen["grid_cols"], chosen["grid_rows"] = c1 - c0, r1 - r0
    return chosen


def delete_region(data: dict, region: TileRegion) -> dict:
    _, remaining = _partition(data, region)
    _check_boundary_lights(data, remaining, region)
    return remaining


# A light outside the region whose wall this edit removes would be
# orphaned; a light already orphaned or on an unknown side is the repair
# dialog's business, not this edit's.
def _check_boundary_lights(before: dict, after: dict, region: TileRegion) -> None:
    for index in range(region.level, region.top):
        edges = lambda level: {edge_key(wall) for wall in level["walls"]}
        removed = edges(before["levels"][index]) - edges(after["levels"][index])
        for light in before["levels"][index].get("lights", []):
            if (
                light["side"] in LIGHT_SIDES
                and not region.contains_cell(light["col"], light["row"])
                and wall_endpoints_for_cell_side(light["col"], light["row"], light["side"]) in removed
            ):
                raise ValueError("A boundary wall holds a light outside the selection. Include the tile on that side too.")


def paste_region(data: dict, block: dict, cell: tuple[int, int], level: int) -> dict:
    col, row = cell
    destination = TileRegion((col, row, col + block["grid_cols"], row + block["grid_rows"]), level, len(block["levels"]))
    expanded = copy.deepcopy(data)
    while len(expanded["levels"]) < destination.top:
        expanded["levels"].append(empty_level(len(expanded["levels"])))
    destination.check_bounds(expanded)
    try:
        _, remaining = _partition(expanded, destination)
    except ValueError as exc:
        raise ValueError(str(exc).replace("The selection crosses", "The destination crosses").replace(
            "Include", "Choose a tile whose block includes")) from exc
    moved = translate_map(block, col, row, level)
    for offset, source in enumerate(moved["levels"]):
        for name in LEVEL_LISTS:
            remaining["levels"][level + offset][name].extend(source[name])
    for name in GLOBAL_LISTS:
        remaining[name].extend(moved[name])
    _check_boundary_lights(expanded, remaining, destination)
    return remaining

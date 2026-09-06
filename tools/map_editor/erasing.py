"""Pure erasure and object picking, in grid units."""

from __future__ import annotations

import copy

from .constants import (
    FLOOR_HIT_KINDS,
    ITEMS_LIST,
    LADDER_SIDES,
    LIGHT_SIDES,
    MODE_ERASE_BARRIERS,
    MODE_ERASE_FLOORS,
    MODE_ERASE_GRASS,
    MODE_ERASE_ITEMS,
    MODE_ERASE_LADDERS,
    MODE_ERASE_LIGHT_BRIDGES,
    MODE_ERASE_LIGHTS,
    MODE_ERASE_NESTED_MAPS,
    MODE_ERASE_PRESSURE_PLATES,
    MODE_ERASE_RAMPS,
    MODE_ERASE_SPAWN_ZONES,
    MODE_ERASE_WALLS,
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
    point_near_wall,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
    wall_endpoints_for_cell_side,
    wall_overlaps_rect,
    zone_contains_cell,
    zone_intersects_rect,
)
from .normalization import edge_key, ladder_key, ladder_spans_level, nested_map_key

Rect = tuple[int, int, int, int]


def cells_outside(entries: list[dict], rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect
    return [e for e in entries if not (c0 <= e["col"] < c1 and r0 <= e["row"] < r1)]


def level_cells_outside(entries: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect
    return [e for e in entries if not (e["level"] == level_idx and c0 <= e["col"] < c1 and r0 <= e["row"] < r1)]


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
    def intersects(ladder):
        if ladder["side"] not in LADDER_SIDES:
            c, r = ladder["col"], ladder["row"]
            return rects_overlap((c, r, c + 1, r + 1), rect)
        return wall_overlaps_rect(
            list(wall_endpoints_for_cell_side(ladder["col"], ladder["row"], ladder["side"])), rect
        )

    return [ladder for ladder in ladders if not ladder_spans_level(ladder, level_idx) or not intersects(ladder)]


def nested_maps_outside(entries: list[dict], level_idx: int, rect: Rect) -> list[dict]:
    c0, r0, c1, r1 = rect

    def end_in_rect(cell: list[int], level: int) -> bool:
        return level == level_idx and c0 <= cell[0] < c1 and r0 <= cell[1] < r1

    return [
        entry
        for entry in entries
        if not (end_in_rect(entry["from"], entry["level"]) or end_in_rect(entry["to"], entry["to_level"]))
    ]


# Lights hang on walls, and grass and items stand on floors: an erase that
# takes the support takes them too, so the map never holds an orphan even
# while canonicalization is withheld for pending repairs.
def lights_on_walls(lights: list[dict], walls: list[dict]) -> list[dict]:
    edges = {edge_key(w) for w in walls}
    return [
        light
        for light in lights
        if light["side"] not in LIGHT_SIDES
        or wall_endpoints_for_cell_side(light["col"], light["row"], light["side"]) in edges
    ]


def standing_on_floors(entries: list[dict], floors: list[dict]) -> list[dict]:
    slabs = {(f["col"], f["row"]) for f in floors}
    return [entry for entry in entries if (entry["col"], entry["row"]) in slabs]


def erase_floors(data: dict, level_idx: int, rect: Rect) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    level["floors"] = cells_outside(level["floors"], rect)
    level["inaccessible_floors"] = cells_outside(level["inaccessible_floors"], rect)
    slabs = [*level["floors"], *level["inaccessible_floors"]]
    level["grass"] = standing_on_floors(level.get("grass", []), slabs)
    after[ITEMS_LIST] = [
        item
        for item in after.get(ITEMS_LIST, [])
        if item["level"] != level_idx or (item["col"], item["row"]) in {(f["col"], f["row"]) for f in level["floors"]}
    ]
    return after


def erase_walls(data: dict, level_idx: int, rect: Rect) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    level["walls"] = edges_outside(level["walls"], rect)
    level["lights"] = lights_on_walls(level.get("lights", []), level["walls"])
    return after


# One record group per erase tool: the lists it clears and how a rectangle
# on a level empties them. A group spanning levels (ramps, ladders, nested
# maps, zones) keeps its level test inside its keep function.
def _keep_level_cells(name: str):
    return lambda data, level_idx, rect: {(level_idx, name): cells_outside(data["levels"][level_idx].get(name, []), rect)}


def _keep_level_edges(name: str):
    return lambda data, level_idx, rect: {(level_idx, name): edges_outside(data["levels"][level_idx].get(name, []), rect)}


def _keep_cells_on_level(name: str):
    return lambda data, level_idx, rect: {(None, name): level_cells_outside(data.get(name, []), level_idx, rect)}


def _keep_floors(data: dict, level_idx: int, rect: Rect) -> dict:
    after = erase_floors(data, level_idx, rect)
    level = after["levels"][level_idx]
    return {
        (level_idx, "floors"): level["floors"],
        (level_idx, "inaccessible_floors"): level["inaccessible_floors"],
        (level_idx, "grass"): level["grass"],
        (None, ITEMS_LIST): after.get(ITEMS_LIST, []),
    }


def _keep_walls(data: dict, level_idx: int, rect: Rect) -> dict:
    level = erase_walls(data, level_idx, rect)["levels"][level_idx]
    return {(level_idx, "walls"): level["walls"], (level_idx, "lights"): level["lights"]}


def _keep_spawn_zones(data: dict, level_idx: int, rect: Rect) -> dict:
    return {(None, name): zones_outside(data[name], level_idx, rect) for name in SPAWN_ZONE_LISTS}


# Keyed by the erase mode; each value is the group's noun for feedback and
# its keep function, which maps `(level or None, list)` to what survives.
ERASE_GROUPS = {
    MODE_ERASE_FLOORS: ("floors", _keep_floors),
    MODE_ERASE_GRASS: ("grass", _keep_level_cells("grass")),
    MODE_ERASE_WALLS: ("walls", _keep_walls),
    MODE_ERASE_BARRIERS: ("barriers", _keep_level_edges("barriers")),
    MODE_ERASE_LIGHT_BRIDGES: ("light bridges", _keep_level_cells("light_bridges")),
    MODE_ERASE_LIGHTS: ("lights", _keep_level_cells("lights")),
    MODE_ERASE_SPAWN_ZONES: ("spawn zones", _keep_spawn_zones),
    MODE_ERASE_ITEMS: ("items", _keep_cells_on_level(ITEMS_LIST)),
    MODE_ERASE_PRESSURE_PLATES: ("plates", _keep_cells_on_level("pressure_plates")),
    MODE_ERASE_RAMPS: ("ramps", lambda data, level_idx, rect: {(None, "ramps"): ramps_outside(data["ramps"], level_idx, rect)}),
    MODE_ERASE_LADDERS: ("ladders", lambda data, level_idx, rect: {(None, "ladders"): ladders_outside(data.get("ladders", []), level_idx, rect)}),
    MODE_ERASE_NESTED_MAPS: (
        "nested maps",
        lambda data, level_idx, rect: {(None, NESTED_MAPS_LIST): nested_maps_outside(data.get(NESTED_MAPS_LIST, []), level_idx, rect)},
    ),
}


# The map with one group's records inside `rect` on `level_idx` erased, or
# None when the rectangle held none of them.
def erase_group_rect(data: dict, mode: str, level_idx: int, rect: Rect) -> dict | None:
    _, keep = ERASE_GROUPS[mode]
    kept = keep(data, level_idx, rect)
    current = {
        (level, name): (data["levels"][level] if level is not None else data).get(name, [])
        for level, name in kept
    }
    if all(len(kept[key]) == len(current[key]) for key in kept):
        return None
    after = copy.deepcopy(data)
    for (level, name), entries in kept.items():
        target = after["levels"][level] if level is not None else after
        target[name] = entries
    return after


def erase_cell_rect(
    data: dict, level_idx: int, start: tuple[int, int], end: tuple[int, int], preserve_floors: bool
) -> dict:
    rect = rect_from_cells(start, end)
    after = erase_walls(data, level_idx, rect)
    # Keep Floors keeps the slabs and what stands on them; everything
    # else in the rectangle goes in both modes.
    if not preserve_floors:
        after = erase_floors(after, level_idx, rect)
        level = after["levels"][level_idx]
        level["light_bridges"] = cells_outside(level.get("light_bridges", []), rect)
        after[ITEMS_LIST] = level_cells_outside(after.get(ITEMS_LIST, []), level_idx, rect)
        after["pressure_plates"] = level_cells_outside(after.get("pressure_plates", []), level_idx, rect)
        after[NESTED_MAPS_LIST] = nested_maps_outside(after.get(NESTED_MAPS_LIST, []), level_idx, rect)
    level = after["levels"][level_idx]
    level["grass"] = cells_outside(level.get("grass", []), rect)
    level["barriers"] = edges_outside(level.get("barriers", []), rect)
    for list_name in SPAWN_ZONE_LISTS:
        after[list_name] = zones_outside(after[list_name], level_idx, rect)
    after["ramps"] = ramps_outside(after["ramps"], level_idx, rect)
    after["ladders"] = ladders_outside(after.get("ladders", []), level_idx, rect)
    return after


def hit_at(data: dict, level_idx: int, px: float, py: float):
    col = int(px // 1)
    row = int(py // 1)
    level = data["levels"][level_idx]

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
    for ladder in data.get("ladders", []):
        if not ladder_spans_level(ladder, level_idx):
            continue
        if ladder["side"] not in LADDER_SIDES:
            if (col, row) == (ladder["col"], ladder["row"]):
                return ("Ladder", ladder_key(ladder))
            continue
        edge = list(wall_endpoints_for_cell_side(ladder["col"], ladder["row"], ladder["side"]))
        if point_near_wall(px, py, edge):
            return ("Ladder", ladder_key(ladder))
    on_level = level_idx
    if any(p["level"] == on_level and (p["col"], p["row"]) == (col, row) for p in data.get("pressure_plates", [])):
        return ("Pressure Plate", (col, row))
    if any(i["level"] == on_level and (i["col"], i["row"]) == (col, row) for i in data.get(ITEMS_LIST, [])):
        return ("Item", (col, row))
    # Walk every zone list in reverse so the most-recently-painted entry
    # wins. SPAWN_ZONE_LISTS is ordered actor → player, so when both zone
    # types share a cell the actor zone is preferred.
    for list_name in SPAWN_ZONE_LISTS:
        for idx in range(len(data[list_name]) - 1, -1, -1):
            zone = data[list_name][idx]
            if zone["level"] == level_idx and zone_contains_cell(zone, col, row):
                return ("Spawn Zone", (list_name, idx))
    for ramp in data["ramps"]:
        lower = ramp["lower_level"]
        if level_idx not in (lower, lower + 1):
            continue
        c0, r0, c1, r1 = ramp_rect(ramp)
        if c0 <= col < c1 and r0 <= row < r1:
            return ("Ramp", (lower, tuple(ramp["low"]), tuple(ramp["high"])))
    if any(b["col"] == col and b["row"] == row for b in level.get("light_bridges", [])):
        return (MODE_LIGHT_BRIDGE, (col, row))
    # Only a nested map's anchor cells are hit targets: whatever lies
    # under its footprint stays clickable.
    for entry in data.get(NESTED_MAPS_LIST, []):
        at_start = entry["level"] == level_idx and entry["from"] == [col, row]
        at_end = entry["to_level"] == level_idx and entry["to"] == [col, row]
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


def erase_hit(data: dict, level_idx: int, hit, preserve_floors: bool = False) -> dict:
    kind, value = hit
    if preserve_floors and kind in FLOOR_HIT_KINDS:
        return copy.deepcopy(data)
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    if kind == MODE_GRASS:
        level["grass"] = [grass for grass in level.get("grass", []) if (grass["col"], grass["row"]) != value]
    elif kind in (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR):
        col, row = value
        return erase_floors(data, level_idx, (col, row, col + 1, row + 1))
    elif kind == MODE_LIGHT_BRIDGE:
        level["light_bridges"] = [
            bridge for bridge in level.get("light_bridges", []) if (bridge["col"], bridge["row"]) != value
        ]
    elif kind == MODE_NESTED_MAP:
        after[NESTED_MAPS_LIST] = [entry for entry in after.get(NESTED_MAPS_LIST, []) if nested_map_key(entry) != value]
    elif kind == "Spawn Zone":
        list_name, target_idx = value
        if 0 <= target_idx < len(after[list_name]):
            del after[list_name][target_idx]
    elif kind == "Light":
        level["lights"] = [
            light for light in level.get("lights", []) if (light["col"], light["row"], light["side"]) != value
        ]
    elif kind == "Pressure Plate":
        after["pressure_plates"] = [
            plate
            for plate in after.get("pressure_plates", [])
            if not (plate["level"] == level_idx and (plate["col"], plate["row"]) == value)
        ]
    elif kind == "Item":
        after[ITEMS_LIST] = [
            item
            for item in after.get(ITEMS_LIST, [])
            if not (item["level"] == level_idx and (item["col"], item["row"]) == value)
        ]
    elif kind == "Wall":
        level["walls"] = [
            wall
            for wall in level["walls"]
            if edge_key(wall) != value
        ]
        level["lights"] = lights_on_walls(level.get("lights", []), level["walls"])
    elif kind == "Barrier":
        level["barriers"] = [
            barrier
            for barrier in level.get("barriers", [])
            if edge_key(barrier) != value
        ]
    elif kind == "Ramp":
        lower, low, high = value
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if (ramp["lower_level"], tuple(ramp["low"]), tuple(ramp["high"])) != (lower, low, high)
        ]
    elif kind == "Ladder":
        after["ladders"] = [ladder for ladder in after.get("ladders", []) if ladder_key(ladder) != value]
    return after

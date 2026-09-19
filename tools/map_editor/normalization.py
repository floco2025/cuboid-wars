"""Map source normalization and placement checks supplied by Rust map_core."""

from __future__ import annotations

from .constants import DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS

from .core import call, tuples
from .geometry import normalized_wall


def empty_level(index: int):
    return call("empty_level", index)


def level_label(level: dict, index: int):
    return call("level_label", level, index)


def empty_map(grid_cols: int = DEFAULT_GRID_COLS, grid_rows: int = DEFAULT_GRID_ROWS):
    return call("empty_map", grid_cols, grid_rows)


def started_map(grid_cols: int, grid_rows: int, material: str):
    return call("started_map", grid_cols, grid_rows, material)


def expand_face_materials(obj: dict):
    return call("expand_face_materials", obj)


def compact_face_materials(faces: dict[str, str]):
    return call("compact_face_materials", faces)


def expand_terrain_materials(obj: dict):
    return call("expand_terrain_materials", obj)


def compact_terrain_materials(materials: dict[str, str]):
    return call("compact_terrain_materials", materials)


def normalize_map(map_data: dict):
    return call("normalize_map", map_data)


def normalize_floor(floor: dict):
    return call("normalize_floor", floor)


def normalize_terrain(cell: dict):
    return call("normalize_terrain", cell)


def normalize_wall(wall: dict):
    return call("normalize_wall", wall)


def normalize_eraser(eraser: dict):
    return call("normalize_eraser", eraser)


def control_fields(entry: dict):
    return call("control_fields", entry)


def normalize_barrier(barrier: dict):
    return call("normalize_barrier", barrier)


def normalize_light_bridge(bridge: dict):
    return call("normalize_light_bridge", bridge)


def normalize_ramp(ramp: dict):
    return call("normalize_ramp", ramp)


def normalize_ladder(ladder: dict):
    return call("normalize_ladder", ladder)


def ladder_key(ladder: dict):
    return tuples(call("ladder_key", ladder))


def ladder_edge_key(ladder: dict):
    return tuples(call("ladder_edge_key", ladder))


def ladders_overlap(a: dict, b: dict):
    return call("ladders_overlap", a, b)


# A placement rule reads its own level and the ramps. The hover ghost asks on
# every repaint, so only those cross the boundary, not the whole document.
def _placement_view(data: dict, level_idx: int) -> dict:
    levels = [level if index == level_idx else {} for index, level in enumerate(data["levels"])]
    return {"levels": levels, "ramps": data["ramps"]}


def item_cell_error(data: dict, level_idx: int, col: int, row: int):
    return call("item_cell_error", _placement_view(data, level_idx), level_idx, col, row)


def plate_cell_error(data: dict, level_idx: int, col: int, row: int):
    return call("plate_cell_error", _placement_view(data, level_idx), level_idx, col, row)


def light_placement_error(data: dict, level_idx: int, col: int, row: int, side: str):
    return call("light_placement_error", _placement_view(data, level_idx), level_idx, col, row, side)


def ladder_spans_level(ladder: dict, level_idx: int):
    return call("ladder_spans_level", ladder, level_idx)


def normalize_nested_map(entry: dict):
    return call("normalize_nested_map", entry)


def nested_map_key(entry: dict):
    return tuples(call("nested_map_key", entry))


def nested_map_spans_level(entry: dict, level_idx: int, level_count: int):
    return call("nested_map_spans_level", entry, level_idx, level_count)


def normalize_light(light: dict):
    return call("normalize_light", light)


# Python, not map_core: the canvas calls this per record on every mouse move (see `grid_int` in core.py).
def edge_key(entry: dict) -> tuple[int, int, int, int]:
    return tuple(normalized_wall([entry.get(key) for key in ("c0", "r0", "c1", "r1")]))


def light_key(light: dict):
    return tuples(call("light_key", light))


def normalize_actor_spawn_zone(zone: dict):
    return call("normalize_actor_spawn_zone", zone)


def normalize_checkpoint(zone: dict):
    return call("normalize_checkpoint", zone)


def normalize_item(item: dict):
    return call("normalize_item", item)


def normalize_pressure_plate(plate: dict):
    return call("normalize_pressure_plate", plate)


def pressure_plate_key(plate: dict):
    return tuples(call("pressure_plate_key", plate))


def actor_zone_key(zone: dict):
    return tuples(call("actor_zone_key", zone))


def checkpoint_key(zone: dict):
    return tuples(call("checkpoint_key", zone))


def zone_key(list_name: str, zone: dict):
    return tuples(call("zone_key", list_name, zone))


def canonicalize_map(map_data: dict):
    return call("canonicalize_map", map_data)


def enforce_ramp_floor_rules(map_data: dict):
    result = call("enforce_ramp_floor_rules", map_data)
    map_data.clear()
    map_data.update(result)

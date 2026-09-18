"""Coordinate families and level operations shared by map editing, clipping, and clipboard blocks."""

from __future__ import annotations

from dataclasses import dataclass

from .constants import ZONE_LISTS
from .nesting import nested_map_shape
from .core import call, shapes

CELL_LISTS = ("floors", "inaccessible_floors", "terrain", "light_bridges", "lights")
EDGE_LISTS = ("walls", "barriers", "erasers")
LEVEL_LISTS = (*CELL_LISTS, *EDGE_LISTS)
GLOBAL_LISTS = (*ZONE_LISTS, "items", "pressure_plates", "ramps", "ladders", "nested_maps")


def record_lists(data: dict):
    for index, level in enumerate(data["levels"]):
        for name in LEVEL_LISTS:
            yield (index, name), level.get(name, [])
    for name in GLOBAL_LISTS:
        yield (None, name), data.get(name, [])


def record_rect(name: str, entry: dict):
    return tuple(call("record_rect", name, entry))


def record_levels(entry: dict, level: int | None = None):
    return tuple(call("record_levels", entry, level))


@dataclass(frozen=True)
class ContentBounds:
    rect: tuple[int, int, int, int]
    first_level: int
    last_level: int


def map_content_bounds(data: dict, *, nested_lookup=None, wall_width_cells=0.0, floor_height_levels=0.0):
    lookup = nested_lookup or (lambda name: nested_map_shape(data.get("nested_geometry", {}).get(name)))
    result = call(
        "map_content_bounds", data, shapes(data.get("nested_maps", []), lookup), wall_width_cells, floor_height_levels
    )
    return ContentBounds(tuple(result["rect"]), result["first_level"], result["last_level"])


def translate_entry(name: str, entry: dict, dc: int = 0, dr: int = 0, dl: int = 0):
    return call("translate_entry", name, entry, dc, dr, dl)


def translate_map(data: dict, dc: int, dr: int, dl: int = 0):
    return call("translate_map", data, dc, dr, dl)


def resize_map_offset(data: dict, cols: int, rows: int, dc: int, dr: int):
    return call("resize_map_offset", data, cols, rows, dc, dr)


def remap_levels(data: dict, pivot: int, *, remove: bool):
    return call("remap_levels", data, pivot, remove)


def element_counts(data: dict) -> dict[str, int]:
    return {
        **{name: sum(len(level.get(name, [])) for level in data["levels"]) for name in LEVEL_LISTS},
        **{name: len(data.get(name, [])) for name in GLOBAL_LISTS},
    }


# What an edit drops, per record list, for the confirmation before it;
# empty when nothing goes.
def dropped_summary(before: dict, after: dict) -> str:
    before_counts, after_counts = element_counts(before), element_counts(after)
    parts = [
        f"{count - after_counts[name]} {name.replace('_', ' ')}"
        for name, count in before_counts.items()
        if count > after_counts[name]
    ]
    return "This will drop:\n  - " + "\n  - ".join(parts) if parts else ""


# The ramps a level inserted at `insert_at` would separate from their top.
def crossing_ramps(map_data: dict, insert_at: int) -> list[dict]:
    return [ramp for ramp in map_data["ramps"] if ramp["lower_level"] + 1 == insert_at]


def insert_level_data(map_data: dict, insert_at: int, *, remove_crossing_ramps: bool = False):
    return call("insert_level_data", map_data, insert_at, remove_crossing_ramps)


def remove_level_data(map_data: dict, removed: int):
    return call("remove_level_data", map_data, removed)


def edit_levels_data(map_data: dict, levels: list[tuple[int | None, str]]):
    return call("edit_levels_data", map_data, levels)


def transform_block(block, operation, definitions):
    result, additions = call("transform_block", block, operation, definitions)
    return result, additions

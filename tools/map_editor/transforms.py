"""Coordinate families shared by map editing, clipping, and clipboard blocks."""

from __future__ import annotations

import copy

from .constants import SPAWN_ZONE_LISTS


CELL_LISTS = ("floors", "inaccessible_floors", "grass", "light_bridges", "lights")
EDGE_LISTS = ("walls", "barriers", "erasers")
LEVEL_LISTS = (*CELL_LISTS, *EDGE_LISTS)
GLOBAL_LISTS = (*SPAWN_ZONE_LISTS, "items", "pressure_plates", "ramps", "ladders", "nested_maps")


def record_lists(data: dict):
    for index, level in enumerate(data["levels"]):
        for name in LEVEL_LISTS:
            yield (index, name), level.get(name, [])
    for name in GLOBAL_LISTS:
        yield (None, name), data.get(name, [])


def record_rect(name: str, entry: dict) -> tuple[int, int, int, int]:
    if name in SPAWN_ZONE_LISTS:
        return entry["cols"][0], entry["rows"][0], entry["cols"][1], entry["rows"][1]
    if name in EDGE_LISTS:
        return (
            min(entry["c0"], entry["c1"]),
            min(entry["r0"], entry["r1"]),
            max(entry["c0"], entry["c1"]),
            max(entry["r0"], entry["r1"]),
        )
    if name in ("ramps", "nested_maps"):
        start, end = (entry["low"], entry["high"]) if name == "ramps" else (entry["from"], entry["to"])
        extra = int(name == "nested_maps")
        return (
            min(start[0], end[0]),
            min(start[1], end[1]),
            max(start[0], end[0]) + extra,
            max(start[1], end[1]) + extra,
        )
    return entry["col"], entry["row"], entry["col"] + 1, entry["row"] + 1


def record_levels(entry: dict, level: int | None = None) -> tuple[int, int]:
    if level is not None:
        return level, level
    if "lower_level" in entry:
        return entry["lower_level"], entry["lower_level"] + entry.get("levels", 1)
    return min(entry["level"], entry.get("to_level", entry["level"])), max(
        entry["level"], entry.get("to_level", entry["level"])
    )


def translate_entry(name: str, entry: dict, dc: int = 0, dr: int = 0, dl: int = 0) -> dict:
    moved = copy.deepcopy(entry)
    if name in SPAWN_ZONE_LISTS:
        moved["cols"] = [c + dc for c in entry["cols"]]
        moved["rows"] = [r + dr for r in entry["rows"]]
    elif name in EDGE_LISTS:
        for key in ("c0", "c1"):
            moved[key] += dc
        for key in ("r0", "r1"):
            moved[key] += dr
    elif name in ("ramps", "nested_maps"):
        for key in ("low", "high") if name == "ramps" else ("from", "to"):
            moved[key] = [entry[key][0] + dc, entry[key][1] + dr]
    else:
        moved["col"] += dc
        moved["row"] += dr
    for key in ("level", "lower_level", "to_level"):
        if key in moved:
            moved[key] += dl
    return moved


def translate_map(data: dict, dc: int, dr: int, dl: int = 0) -> dict:
    moved = copy.deepcopy(data)
    for (_, name), entries in record_lists(moved):
        entries[:] = [translate_entry(name, entry, dc, dr, dl) for entry in entries]
    return moved


def resize_map_data(data: dict, cols: int, rows: int, anchor_x: int, anchor_y: int) -> dict:
    dc = (cols - data["grid_cols"]) * anchor_x // 2
    dr = (rows - data["grid_rows"]) * anchor_y // 2
    moved = translate_map(data, dc, dr)
    moved["grid_cols"], moved["grid_rows"] = cols, rows
    for (_, name), entries in record_lists(moved):
        kept = []
        for entry in entries:
            c0, r0, c1, r1 = record_rect(name, entry)
            if name in SPAWN_ZONE_LISTS:
                c0, r0, c1, r1 = max(0, c0), max(0, r0), min(cols, c1), min(rows, r1)
                if c0 >= c1 or r0 >= r1:
                    continue
                entry["cols"], entry["rows"] = [c0, c1], [r0, r1]
            if 0 <= c0 <= c1 <= cols and 0 <= r0 <= r1 <= rows:
                kept.append(entry)
        entries[:] = kept
    return moved


def remap_levels(data: dict, pivot: int, *, remove: bool) -> dict:
    moved = copy.deepcopy(data)
    for name in GLOBAL_LISTS:
        kept = []
        for entry in moved.get(name, []):
            lower, upper = record_levels(entry)
            if remove and lower <= pivot <= upper:
                continue
            if name == "ladders" and lower < pivot <= upper:
                entry["levels"] += 1
            for key in ("level", "lower_level", "to_level"):
                if key in entry and entry[key] >= pivot:
                    entry[key] += -1 if remove else 1
            kept.append(entry)
        moved[name] = kept
    return moved

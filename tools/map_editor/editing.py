"""Pure map edits; dialogs and selection state belong to their callers."""

from __future__ import annotations

import copy

from .constants import FACES, SPAWN_ZONE_LISTS
from .geometry import ramp_rect, rects_overlap, wall_segments_between, zone_intersects_rect
from .normalization import edge_key, pressure_plate_key
from .transforms import record_rect


def replace_records(data: dict, name: str, entries: list[dict], level: int | None = None) -> dict:
    after = copy.deepcopy(data)
    target = after if level is None else after["levels"][level]
    target[name] = copy.deepcopy(entries)
    return after


def update_records(data: dict, name: str, predicate, values: dict, level: int | None = None) -> dict:
    target = data if level is None else data["levels"][level]
    return replace_records(
        data, name, [{**entry, **values} if predicate(entry) else entry for entry in target.get(name, [])], level
    )


def paint_floors(data: dict, level_idx: int, rect: tuple, material: str, *, blocked: bool = False) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    added, removed = ("inaccessible_floors", "floors") if blocked else ("floors", "inaccessible_floors")
    existing = {(f["col"], f["row"]): f for f in level[added]}
    c0, r0, c1, r1 = rect
    cells = {(c, r) for r in range(r0, r1) for c in range(c0, c1)}
    for col, row in sorted(cells):
        existing.setdefault((col, row), {"col": col, "row": row, **dict.fromkeys(FACES, material)})
    level[added] = list(existing.values())
    level[removed] = [f for f in level[removed] if (f["col"], f["row"]) not in cells]
    if blocked:
        for name in SPAWN_ZONE_LISTS:
            after[name] = [z for z in after[name] if z["level"] != level_idx or not zone_intersects_rect(z, rect)]
    return after


def paint_grass(data: dict, level_idx: int, rect: tuple) -> dict:
    level = data["levels"][level_idx]
    slabs = {(f["col"], f["row"]) for name in ("floors", "inaccessible_floors") for f in level[name]}
    grass = {(g["col"], g["row"]) for g in level["grass"]}
    c0, r0, c1, r1 = rect
    grass.update((c, r) for c, r in slabs if c0 <= c < c1 and r0 <= r < r1)
    return replace_records(data, "grass", [{"col": c, "row": r} for c, r in sorted(grass)], level_idx)


def paint_edges(
    data: dict, level_idx: int, start: tuple, end: tuple, *, material: str | None = None, kind: str | None = None
) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    name = "walls" if material is not None else "barriers"

    walls = {edge_key(w) for w in level["walls"]}
    existing = {edge_key(e): e for e in level[name]}
    for endpoints in wall_segments_between(start, end):
        key = tuple(endpoints)
        entry = dict(zip(("c0", "r0", "c1", "r1"), endpoints))
        if material is not None:
            existing.setdefault(key, {**entry, **dict.fromkeys(FACES, material)})
        elif key not in walls:
            existing[key] = {**entry, "kind": kind}
    level[name] = list(existing.values())
    if material is not None:
        level["barriers"] = [b for b in level["barriers"] if edge_key(b) not in existing]
    return after


def paint_bridges(data: dict, level_idx: int, rect: tuple, kind: str) -> dict:
    c0, r0, c1, r1 = rect
    existing = {(b["col"], b["row"]): b for b in data["levels"][level_idx].get("light_bridges", [])}
    existing.update({(c, r): {"col": c, "row": r, "kind": kind} for r in range(r0, r1) for c in range(c0, c1)})
    return replace_records(data, "light_bridges", list(existing.values()), level_idx)


def place_ramp(data: dict, ramp: dict) -> dict:
    lower = ramp["lower_level"]
    rect = ramp_rect(ramp)
    kept = [r for r in data["ramps"] if abs(r["lower_level"] - lower) > 1 or not rects_overlap(rect, ramp_rect(r))]
    return replace_records(data, "ramps", [*kept, ramp])


def place_plate(data: dict, plate: dict, *, replacing: tuple | None = None) -> dict:
    key = pressure_plate_key(plate)
    existing = [p for p in data["pressure_plates"] if pressure_plate_key(p) != replacing]
    if any(pressure_plate_key(p) == key for p in existing):
        raise ValueError("That plate purpose is already on this tile.")
    return replace_records(data, "pressure_plates", [*existing, plate])


def material_values(entries: list[dict]) -> dict[str, str | None]:
    result = {}
    for face in FACES:
        values = {entry.get(face) for entry in entries}
        result[face] = next(iter(values)) if len(values) == 1 else None
    return result


def top_left_materials(entries: list[dict], name: str) -> dict[str, str | None]:
    def spatial_order(entry):
        c0, r0, c1, r1 = record_rect(name, entry)
        return r0, c0, r1, c1

    first = min(entries, key=spatial_order)
    return {face: first.get(face) for face in FACES}

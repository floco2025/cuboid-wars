"""Pure map edits; dialogs and selection state belong to their callers."""

from __future__ import annotations

import copy

from .constants import FACES, TERRAIN_FACES, ZONE_LISTS
from .geometry import ramp_rect, rects_overlap, wall_segments_between, zone_intersects_rect
from .normalization import edge_key, pressure_plate_key
from .transforms import record_rect


def replace_records(data: dict, name: str, entries: list[dict], level: int | None = None) -> dict:
    after = copy.deepcopy(data)
    target = after if level is None else after["levels"][level]
    target[name] = copy.deepcopy(entries)
    return after


def merge_record(entry: dict, values: dict) -> dict:
    merged = {**entry, **values}
    if values.get("switch", "") is None:
        del merged["switch"]
    if "switch" in values and "switch" not in merged:
        merged.pop("switch_inverted", None)
    if values.get("until_checkpoint", 0) is None:
        merged.pop("until_checkpoint", None)
        merged.pop("on_checkpoint", None)
    return merged


def update_records(data: dict, name: str, predicate, values: dict, level: int | None = None) -> dict:
    target = data if level is None else data["levels"][level]
    return replace_records(
        data,
        name,
        [merge_record(entry, values) if predicate(entry) else entry for entry in target.get(name, [])],
        level,
    )


def placement_materials(material, faces=FACES):
    return (
        {face: material.get(face, next(iter(material.values()), "")) for face in faces}
        if isinstance(material, dict)
        else dict.fromkeys(faces, material)
    )


def paint_floors(
    data: dict, level_idx: int, rect: tuple, material: str | dict[str, str], *, blocked: bool = False
) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    added, removed = ("inaccessible_floors", "floors") if blocked else ("floors", "inaccessible_floors")
    existing = {(f["col"], f["row"]): f for f in level[added]}
    c0, r0, c1, r1 = rect
    cells = {(c, r) for r in range(r0, r1) for c in range(c0, c1)}
    for col, row in sorted(cells, key=lambda cell: (cell[1], cell[0])):
        existing.setdefault((col, row), {"col": col, "row": row, **placement_materials(material)})
    level[added] = list(existing.values())
    level[removed] = [f for f in level[removed] if (f["col"], f["row"]) not in cells]
    level["terrain"] = [f for f in level["terrain"] if (f["col"], f["row"]) not in cells]
    if blocked:
        for name in ZONE_LISTS:
            after[name] = [z for z in after[name] if z["level"] != level_idx or not zone_intersects_rect(z, rect)]
    return after


def paint_terrain(data: dict, level_idx: int, rect: tuple, material: str | dict[str, str]) -> dict:
    after = copy.deepcopy(data)
    level = after["levels"][level_idx]
    terrain = {(cell["col"], cell["row"]): cell for cell in level["terrain"]}
    c0, r0, c1, r1 = rect
    cells = {(c, r) for r in range(r0, r1) for c in range(c0, c1)}
    for col, row in sorted(cells, key=lambda cell: (cell[1], cell[0])):
        terrain.setdefault(
            (col, row),
            {"col": col, "row": row, **placement_materials(material, TERRAIN_FACES)},
        )
    level["terrain"] = list(terrain.values())
    level["floors"] = [f for f in level["floors"] if (f["col"], f["row"]) not in cells]
    level["inaccessible_floors"] = [f for f in level["inaccessible_floors"] if (f["col"], f["row"]) not in cells]
    return after


def paint_edges(
    data: dict,
    level_idx: int,
    start: tuple,
    end: tuple,
    *,
    material: str | dict[str, str] | None = None,
    kind: str | None = None,
    controls: dict | None = None,
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
            existing.setdefault(key, {**entry, **placement_materials(material)})
        elif key not in walls:
            existing[key] = {**entry, "kind": kind, **(controls or {})}
    level[name] = list(existing.values())
    if material is not None:
        level["barriers"] = [b for b in level["barriers"] if edge_key(b) not in existing]
    return after


def paint_erasers(data: dict, level_idx: int, start: tuple, end: tuple) -> dict:
    existing = {edge_key(e): e for e in data["levels"][level_idx].get("erasers", [])}
    for endpoints in wall_segments_between(start, end):
        existing[tuple(endpoints)] = dict(zip(("c0", "r0", "c1", "r1"), endpoints))
    return replace_records(data, "erasers", list(existing.values()), level_idx)


def paint_bridges(data: dict, level_idx: int, rect: tuple, kind: str, controls: dict | None = None) -> dict:
    c0, r0, c1, r1 = rect
    existing = {(b["col"], b["row"]): b for b in data["levels"][level_idx].get("light_bridges", [])}
    existing.update(
        {(c, r): {"col": c, "row": r, "kind": kind, **(controls or {})} for r in range(r0, r1) for c in range(c0, c1)}
    )
    return replace_records(data, "light_bridges", list(existing.values()), level_idx)


# A new ramp replaces the ones it could not coexist with: those sharing cells
# with it over more than the level one ends on and the other starts from.
def place_ramp(data: dict, ramp: dict) -> dict:
    lower, upper = ramp["lower_level"], ramp["lower_level"] + ramp["levels"]
    rect = ramp_rect(ramp)
    kept = [
        other
        for other in data["ramps"]
        if not (
            lower < other["lower_level"] + other["levels"]
            and other["lower_level"] < upper
            and rects_overlap(rect, ramp_rect(other))
        )
    ]
    return replace_records(data, "ramps", [*kept, ramp])


def place_plate(data: dict, plate: dict, *, replacing: tuple | None = None) -> dict:
    cell = (plate["level"], plate["col"], plate["row"])
    existing = [p for p in data["pressure_plates"] if pressure_plate_key(p) != replacing]
    if any((p["level"], p["col"], p["row"]) == cell for p in existing):
        raise ValueError("There is already a pressure plate on this tile.")
    return replace_records(data, "pressure_plates", [*existing, plate])


def material_values(entries: list[dict], faces=FACES) -> dict[str, str | None]:
    result = {}
    for face in faces:
        values = {entry.get(face) for entry in entries}
        result[face] = next(iter(values)) if len(values) == 1 else None
    return result


def top_left_materials(entries: list[dict], name: str, faces=FACES) -> dict[str, str | None]:
    def spatial_order(entry):
        c0, r0, c1, r1 = record_rect(name, entry)
        return r0, c0, r1, c1

    first = min(entries, key=spatial_order)
    return {face: first.get(face) for face in faces}

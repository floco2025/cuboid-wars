"""Map-data normalization, canonical ordering, and dedupe."""

from __future__ import annotations

import copy
import math

from .constants import (
    ACTOR_ZONE_LIST,
    CHECKPOINT_LIST,
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    FACES,
    ITEM_KEY_TYPE,
    LADDER_SIDES,
    LIGHT_SIDES,
    MAP_NAME_RE,
)
from .geometry import normalized_wall, ramp_cells, ramp_cells_on_level, wall_endpoints_for_cell_side


def empty_level(index: int) -> dict:
    return {
        "name": f"Level {index}",
        "floors": [],
        "inaccessible_floors": [],
        "grass": [],
        "walls": [],
        "barriers": [],
        "erasers": [],
        "light_bridges": [],
        "lights": [],
    }


def level_label(level: dict, index: int) -> str:
    name = level.get("name")
    return f"Level {index}" if not name else f"Level {index} ({name})"


def empty_map(grid_cols: int = DEFAULT_GRID_COLS, grid_rows: int = DEFAULT_GRID_ROWS) -> dict:
    # No seeded actor zone: there's no default kind to give it. Users paint
    # actor zones explicitly and pick a kind in the dialog.
    # The player-spawn-zone seed in the top-left guarantees the map is
    # save-valid out of the box (at least one player spawn zone is required).
    return {
        "grid_cols": grid_cols,
        "grid_rows": grid_rows,
        "actor_spawn_zones": [],
        "player_spawn_zones": [
            {"level": 0, "cols": [0, min(2, grid_cols)], "rows": [0, min(2, grid_rows)]},
        ],
        "checkpoints": [],
        "items": [],
        "pressure_plates": [],
        "levels": [empty_level(0)],
        "ramps": [],
        "ladders": [],
        "nested_maps": [],
    }


def expand_face_materials(obj: dict) -> dict[str, str]:
    """Expand `all` shorthand into six explicit face materials. Faces not
    explicitly set fall back to `all` (or to any other face value if `all` is
    absent). Used when reading per-segment material data from JSON."""
    fallback = obj.get("all")
    if fallback is None:
        fallback = next((obj[face] for face in FACES if face in obj), None)
    if fallback is None:
        fallback = ""
    return {face: obj.get(face, fallback) for face in FACES}


def compact_face_materials(faces: dict[str, str]) -> dict:
    """Pack six face materials into the on-disk `all` + overrides shape.
    Picks the most-common face value as `all`; ties broken alphabetically for
    deterministic output."""
    counts: dict[str, int] = {}
    for face in FACES:
        if face in faces:
            counts[faces[face]] = counts.get(faces[face], 0) + 1
    if not counts:
        return {}
    best_count = max(counts.values())
    most_common = sorted(name for name, count in counts.items() if count == best_count)[0]
    if best_count <= 1:
        return {face: faces[face] for face in FACES if face in faces}
    out = {"all": most_common}
    for face in FACES:
        if face in faces and faces[face] != most_common:
            out[face] = faces[face]
    return out


def normalize_map(map_data: dict) -> dict:
    cols = int(map_data.get("grid_cols", DEFAULT_GRID_COLS))
    rows = int(map_data.get("grid_rows", DEFAULT_GRID_ROWS))
    actor_spawn_zones = [normalize_actor_spawn_zone(z) for z in map_data.get("actor_spawn_zones", [])]
    player_spawn_zones = [normalize_player_spawn_zone(z) for z in map_data.get("player_spawn_zones", [])]
    items = [normalize_item(i) for i in map_data.get("items", [])]
    pressure_plates = [normalize_pressure_plate(p) for p in map_data.get("pressure_plates", [])]
    levels = []
    for idx, level in enumerate(map_data.get("levels", [])):
        levels.append(
            {
                "name": str(level.get("name") or f"Level {idx}"),
                "floors": [normalize_floor(f) for f in level.get("floors", [])],
                "inaccessible_floors": [normalize_floor(f) for f in level.get("inaccessible_floors", [])],
                "grass": [normalize_grass(g) for g in level.get("grass", [])],
                "walls": [normalize_wall(w) for w in level.get("walls", [])],
                "barriers": [normalize_barrier(b) for b in level.get("barriers", [])],
                "erasers": [normalize_eraser(e) for e in level.get("erasers", [])],
                "light_bridges": [normalize_light_bridge(b) for b in level.get("light_bridges", [])],
                "lights": [normalize_light(l) for l in level.get("lights", [])],
            }
        )
    if not levels:
        levels = [empty_level(0)]

    ramps = [normalize_ramp(r) for r in map_data.get("ramps", [])]
    ladders = [normalize_ladder(l) for l in map_data.get("ladders", [])]
    nested_maps = [normalize_nested_map(n) for n in map_data.get("nested_maps", [])]
    return {
        **{key: copy.deepcopy(map_data[key]) for key in ("switch_kinds", "fireworks", "_settings") if key in map_data},
        "grid_cols": cols,
        "grid_rows": rows,
        "actor_spawn_zones": actor_spawn_zones,
        "player_spawn_zones": player_spawn_zones,
        "checkpoints": [
            {**normalize_player_spawn_zone(z), "type": str(z.get("type", ""))} for z in map_data.get("checkpoints", [])
        ],
        "items": items,
        "pressure_plates": pressure_plates,
        "levels": levels,
        "ramps": ramps,
        "ladders": ladders,
        "nested_maps": nested_maps,
        **(
            {"nested_geometry": {name: normalize_map(data) for name, data in map_data["nested_geometry"].items()}}
            if "nested_geometry" in map_data
            else {}
        ),
    }


def normalize_floor(floor: dict) -> dict:
    return {
        "col": int(floor["col"]),
        "row": int(floor["row"]),
        **expand_face_materials(floor),
    }


def normalize_grass(cell: dict) -> dict:
    return {"col": int(cell["col"]), "row": int(cell["row"])}


def normalize_wall(wall: dict) -> dict:
    return {
        "c0": int(wall["c0"]),
        "r0": int(wall["r0"]),
        "c1": int(wall["c1"]),
        "r1": int(wall["r1"]),
        **expand_face_materials(wall),
    }


def normalize_eraser(eraser: dict) -> dict:
    return {key: int(eraser[key]) for key in ("c0", "r0", "c1", "r1")}


def control_fields(entry: dict) -> dict:
    return {key: copy.deepcopy(entry[key]) for key in ("switch", "switch_inverted") if key in entry}


def normalize_barrier(barrier: dict) -> dict:
    kind = str(barrier.get("kind", ""))
    return {
        "c0": int(barrier["c0"]),
        "r0": int(barrier["r0"]),
        "c1": int(barrier["c1"]),
        "r1": int(barrier["r1"]),
        "kind": kind,
        **control_fields(barrier),
    }


def normalize_light_bridge(bridge: dict) -> dict:
    return {
        "col": int(bridge["col"]),
        "row": int(bridge["row"]),
        "kind": str(bridge.get("kind", "")),
        **control_fields(bridge),
    }


def normalize_ramp(ramp: dict) -> dict:
    return {
        "low": [int(ramp["low"][0]), int(ramp["low"][1])],
        "high": [int(ramp["high"][0]), int(ramp["high"][1])],
        "lower_level": int(ramp["lower_level"]),
        **expand_face_materials(ramp),
    }


def normalize_ladder(ladder: dict) -> dict:
    side = str(ladder.get("side", "N")).upper()
    return {
        "lower_level": int(ladder.get("lower_level", 0)),
        "col": int(ladder["col"]),
        "row": int(ladder["row"]),
        "side": side,
        "levels": int(ladder.get("levels", 1)),
    }


def ladder_key(ladder: dict) -> tuple:
    return (ladder["lower_level"], ladder["row"], ladder["col"], ladder["side"], ladder["levels"])


def ladder_edge_key(ladder: dict) -> tuple:
    # Undirected: an edge holds at most one ladder — a mirrored pair would
    # put two ladders' geometry on the same edge, so it counts as the same
    # ladder twice even though only the front side climbs.
    return wall_endpoints_for_cell_side(ladder["col"], ladder["row"], ladder["side"])


def ladders_overlap(a: dict, b: dict) -> bool:
    """Whether two ladders put geometry on the same edge on any storey."""
    return (
        a["side"] in LADDER_SIDES
        and b["side"] in LADDER_SIDES
        and ladder_edge_key(a) == ladder_edge_key(b)
        and a["lower_level"] < b["lower_level"] + b["levels"]
        and b["lower_level"] < a["lower_level"] + a["levels"]
    )


# Where a record may stand, as the server accepts it: each returns why a
# cell is refused, or None.


# `has_floor && !has_ramp`: a regular floor outside any lower-level ramp
# footprint, so a ramp's upper storey stays placeable.
def item_cell_error(data: dict, level_idx: int, col: int, row: int) -> str | None:
    if (col, row) not in {(f["col"], f["row"]) for f in data["levels"][level_idx]["floors"]}:
        return f"[{col}, {row}] has no regular floor"
    if any(ramp["lower_level"] == level_idx and (col, row) in ramp_cells(ramp) for ramp in data["ramps"]):
        return f"[{col}, {row}] is inside a ramp footprint"
    return None


# A slab (regular or blocked floor) outside every ramp footprint on the level.
def plate_cell_error(data: dict, level_idx: int, col: int, row: int) -> str | None:
    level = data["levels"][level_idx]
    if (col, row) not in {(f["col"], f["row"]) for name in ("floors", "inaccessible_floors") for f in level[name]}:
        return f"[{col}, {row}] has no floor"
    if (col, row) in ramp_cells_on_level(data["ramps"], level_idx):
        return f"[{col}, {row}] is inside a ramp footprint"
    return None


# A wall on that side of a cell outside every ramp footprint, holding no light yet.
def light_placement_error(data: dict, level_idx: int, col: int, row: int, side: str) -> str | None:
    level = data["levels"][level_idx]
    if wall_endpoints_for_cell_side(col, row, side) not in {edge_key(w) for w in level["walls"]}:
        return f"No wall on the {side} side of cell [{col}, {row}]."
    if (col, row) in ramp_cells_on_level(data["ramps"], level_idx):
        return f"Cannot place a light inside a ramp footprint ([{col}, {row}])."
    if any((l["col"], l["row"], l["side"]) == (col, row, side) for l in level["lights"]):
        return f"There is already a light on the {side} side of cell [{col}, {row}]; right-click it to erase."
    return None


def ladder_spans_level(ladder: dict, level_idx: int) -> bool:
    return ladder["lower_level"] <= level_idx <= ladder["lower_level"] + ladder["levels"]


def normalize_nested_map(entry: dict) -> dict:
    level = int(entry.get("level", 0))
    normalized = {
        "map": str(entry.get("map", "")),
        "level": level,
        "from": [int(entry["from"][0]), int(entry["from"][1])],
        "to": [int(entry["to"][0]), int(entry["to"][1])],
        "to_level": int(entry.get("to_level", level)),
        "travel_secs": float(entry.get("travel_secs", 2.0)),
        "pause_secs": float(entry.get("pause_secs", 0.0)),
        "phase_secs": float(entry.get("phase_secs", 0.0)),
        "from_nudge": [float(axis) for axis in entry.get("from_nudge", (0.0, 0.0, 0.0))],
        "to_nudge": [float(axis) for axis in entry.get("to_nudge", (0.0, 0.0, 0.0))],
    }
    normalized.update(control_fields(entry))
    return normalized


def nested_map_key(entry: dict) -> tuple:
    return (entry["level"], tuple(entry["from"]), entry["to_level"], tuple(entry["to"]), entry["map"])


def nested_map_spans_level(entry: dict, level_idx: int, level_count: int) -> bool:
    # The nested map's own storeys sit on top of the storey each end rests on.
    lowest = min(entry["level"], entry["to_level"])
    highest = max(entry["level"], entry["to_level"]) + max(1, level_count) - 1
    return lowest <= level_idx <= highest


def normalize_light(light: dict) -> dict:
    side = str(light.get("side", "N")).upper()
    return {
        "col": int(light["col"]),
        "row": int(light["row"]),
        "side": side,
        "kind": light.get("kind", ""),
    }


# A wall's or barrier's grid edge, endpoint order normalized.
def edge_key(entry: dict) -> tuple[int, int, int, int]:
    return tuple(normalized_wall([entry["c0"], entry["r0"], entry["c1"], entry["r1"]]))


def light_key(light: dict) -> tuple:
    return (light["row"], light["col"], light["side"])


def _normalize_zone_rect(zone: dict) -> dict:
    cols = zone.get("cols") or [0, 0]
    rows = zone.get("rows") or [0, 0]
    return {
        "level": int(zone.get("level", 0)),
        "cols": [int(cols[0]), int(cols[1])],
        "rows": [int(rows[0]), int(rows[1])],
    }


def normalize_actor_spawn_zone(zone: dict) -> dict:
    kind = str(zone.get("kind", ""))
    try:
        count = int(zone.get("count", 0))
    except (TypeError, ValueError):
        count = 0
    normalized = {**_normalize_zone_rect(zone), "kind": kind, "count": count}
    if zone.get("levels", 1) != 1:
        normalized["levels"] = copy.deepcopy(zone["levels"])
    if zone.get("roam_distance", 0.0) != 0.0:
        normalized["roam_distance"] = copy.deepcopy(zone["roam_distance"])
    if "respawn_secs" in zone:
        normalized["respawn_secs"] = copy.deepcopy(zone["respawn_secs"])
    normalized.update(control_fields(zone))
    return normalized


def normalize_player_spawn_zone(zone: dict) -> dict:
    normalized = _normalize_zone_rect(zone)
    if zone.get("levels", 1) != 1:
        normalized["levels"] = copy.deepcopy(zone["levels"])
    return normalized


def normalize_item(item: dict) -> dict:
    out = {
        "level": int(item.get("level", 0)),
        "col": int(item.get("col", 0)),
        "row": int(item.get("row", 0)),
        "type": str(item.get("type", "")),
    }
    if out["type"] == ITEM_KEY_TYPE or "kind" in item:
        out["kind"] = str(item.get("kind", ""))
    return out


# A missing switch is kept missing for the validator to flag rather than
# invented.
def normalize_pressure_plate(plate: dict) -> dict:
    normalized = {
        "level": int(plate.get("level", 0)),
        "col": int(plate.get("col", 0)),
        "row": int(plate.get("row", 0)),
    }
    if "switch" in plate:
        normalized["switch"] = str(plate["switch"])
    return normalized


def pressure_plate_key(plate: dict) -> tuple:
    return (plate["level"], plate["row"], plate["col"], plate.get("switch", ""))


def _numeric_zone_key(value) -> tuple:
    if isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value):
        return (0, value)
    # Invalid authored values must survive normalization so the issues dock can report them.
    return (1, type(value).__name__, repr(value))


def _control_zone_key(value) -> tuple:
    return (type(value).__name__, value if isinstance(value, (str, bool)) else repr(value))


def actor_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        _numeric_zone_key(zone.get("levels", 1)),
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
        zone["kind"],
        zone["count"],
        _control_zone_key(zone.get("switch")),
        _control_zone_key(zone.get("switch_inverted", False)),
        _numeric_zone_key(zone.get("roam_distance", 0.0)),
        (0,) if zone.get("respawn_secs") is None else (1, _numeric_zone_key(zone["respawn_secs"])),
    )


def player_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        _numeric_zone_key(zone.get("levels", 1)),
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
    )


# Two checkpoints may share a rectangle and differ by type, so the type is
# part of a checkpoint's identity, in selection as in canonicalization.
def checkpoint_key(zone: dict) -> tuple:
    return (*player_zone_key(zone), zone["type"])


def zone_key(list_name: str, zone: dict) -> tuple:
    if list_name == ACTOR_ZONE_LIST:
        return actor_zone_key(zone)
    if list_name == CHECKPOINT_LIST:
        return checkpoint_key(zone)
    return player_zone_key(zone)


def _dedupe_sorted(zones: list[dict], key_fn) -> list[dict]:
    seen = set()
    out = []
    for zone in sorted(zones, key=key_fn):
        k = key_fn(zone)
        if k in seen:
            continue
        seen.add(k)
        out.append(zone)
    return out


def canonicalize_map(map_data: dict) -> dict:
    b = normalize_map(copy.deepcopy(map_data))
    if isinstance(b.get("fireworks"), dict):
        b["fireworks"] = {key: value for key, value in b["fireworks"].items() if key != "switch_inverted"}
    # Sorted before the floor rules, which resolve stacked ramps in list
    # order: the result must not depend on the file's order.
    b["ramps"] = sorted(b["ramps"], key=lambda r: (r["lower_level"], tuple(r["low"]), tuple(r["high"])))
    enforce_ramp_floor_rules(b)
    b["actor_spawn_zones"] = _dedupe_sorted(b["actor_spawn_zones"], actor_zone_key)
    b["player_spawn_zones"] = _dedupe_sorted(b["player_spawn_zones"], player_zone_key)
    b["checkpoints"] = _dedupe_sorted(b["checkpoints"], checkpoint_key)
    b["pressure_plates"] = _dedupe_sorted(b["pressure_plates"], pressure_plate_key)
    # Ramp footprints occupy cells on both the lower and upper level of each
    # ramp. Lights are not allowed inside any of those cells.
    ramp_cells_by_level: list[set[tuple[int, int]]] = [set() for _ in b["levels"]]
    for ramp in b["ramps"]:
        for cell in ramp_cells(ramp):
            for level in (ramp["lower_level"], ramp["lower_level"] + 1):
                if 0 <= level < len(ramp_cells_by_level):
                    ramp_cells_by_level[level].add(cell)
    for level_idx, level in enumerate(b["levels"]):
        # Dedupe by (col, row); later entries win when the same position is
        # painted twice, so the user's most recent paint stays.
        level["floors"] = _dedupe_floors(level["floors"])
        floor_keys = {(f["col"], f["row"]) for f in level["floors"]}
        level["inaccessible_floors"] = [
            f for f in _dedupe_floors(level["inaccessible_floors"]) if (f["col"], f["row"]) not in floor_keys
        ]
        ramp_set = ramp_cells_by_level[level_idx]
        # Grass only survives on slab cells (floor or inaccessible floor)
        # outside ramp footprints, so erasing a floor drops its grass in the
        # same canonicalize pass — the two can never desync.
        slab_keys = floor_keys | {(f["col"], f["row"]) for f in level["inaccessible_floors"]}
        level["grass"] = [
            g
            for g in _dedupe_floors(level["grass"])
            if (g["col"], g["row"]) in slab_keys and (g["col"], g["row"]) not in ramp_set
        ]
        level["walls"] = _dedupe_edges(level["walls"])
        level["erasers"] = _dedupe_edges(level.get("erasers", []))
        wall_endpoints_set = {edge_key(w) for w in level["walls"]}
        # Drop barriers that share an edge with a wall on the same level so
        # canonical files satisfy the Rust loader's conflict rule.
        level["barriers"] = [
            b for b in _dedupe_edges(level.get("barriers", [])) if edge_key(b) not in wall_endpoints_set
        ]
        level["light_bridges"] = _dedupe_floors(level.get("light_bridges", []))
        cols, rows = b["grid_cols"], b["grid_rows"]
        in_bounds_lights = [
            l
            for l in level.get("lights", [])
            if 0 <= l["col"] < cols
            and 0 <= l["row"] < rows
            and l["side"] in LIGHT_SIDES
            and wall_endpoints_for_cell_side(l["col"], l["row"], l["side"]) in wall_endpoints_set
            and (l["col"], l["row"]) not in ramp_set
        ]
        level["lights"] = _dedupe_lights(in_bounds_lights)

    # Items only survive where the server accepts them, so erasing a floor
    # (or laying a ramp) drops its item in the same canonicalize pass.
    items_by_cell: dict[tuple[int, int, int], dict] = {}
    for item in b["items"]:
        level_idx = item["level"]
        if not (0 <= level_idx < len(b["levels"])) or item_cell_error(b, level_idx, item["col"], item["row"]):
            continue
        # Later entries win when the same cell holds two items, so the
        # user's most recent placement stays.
        items_by_cell[(level_idx, item["col"], item["row"])] = item
    b["items"] = [items_by_cell[k] for k in sorted(items_by_cell.keys(), key=lambda k: (k[0], k[2], k[1]))]

    # Ladders: drop out-of-bounds anchors and spans past the top level (both
    # hard errors in the Rust loader), then keep the first ladder per
    # overlapping same-edge span — the loader rejects overlaps.
    cols, rows = b["grid_cols"], b["grid_rows"]
    in_bounds_ladders = [
        l
        for l in b["ladders"]
        if 0 <= l["col"] < cols
        and 0 <= l["row"] < rows
        and l["side"] in LADDER_SIDES
        and l["levels"] >= 1
        and l["lower_level"] >= 0
        and l["lower_level"] + l["levels"] < len(b["levels"])
    ]
    kept_ladders: list[dict] = []
    for ladder in sorted(in_bounds_ladders, key=ladder_key):
        if not any(ladders_overlap(ladder, other) for other in kept_ladders):
            kept_ladders.append(ladder)
    b["ladders"] = kept_ladders

    # Nested maps: drop ones outside the grid or the level range, or with
    # an unsafe name (all hard errors in the Rust loader); one per starting
    # cell and level, the later entry winning like floors.
    level_count = len(b["levels"])
    by_start: dict[tuple, dict] = {}
    for entry in b["nested_maps"]:
        cells_ok = all(0 <= c < cols and 0 <= r < rows for c, r in (entry["from"], entry["to"]))
        levels_ok = 0 <= entry["level"] < level_count and 0 <= entry["to_level"] < level_count
        if cells_ok and levels_ok and MAP_NAME_RE.match(entry["map"]):
            by_start[(entry["level"], tuple(entry["from"]))] = entry
    b["nested_maps"] = sorted(by_start.values(), key=nested_map_key)
    return b


def _dedupe_floors(floors: list[dict]) -> list[dict]:
    by_pos: dict[tuple[int, int], dict] = {}
    for floor in floors:
        by_pos[(floor["col"], floor["row"])] = floor
    return [by_pos[k] for k in sorted(by_pos.keys(), key=lambda p: (p[1], p[0]))]


def _dedupe_edges(walls: list[dict]) -> list[dict]:
    by_edge: dict[tuple[int, int, int, int], dict] = {}
    for wall in walls:
        c0, r0, c1, r1 = normalized_wall([wall["c0"], wall["r0"], wall["c1"], wall["r1"]])
        by_edge[(c0, r0, c1, r1)] = {**wall, "c0": c0, "r0": r0, "c1": c1, "r1": r1}
    return [by_edge[k] for k in sorted(by_edge.keys())]


def _dedupe_lights(lights: list[dict]) -> list[dict]:
    by_key: dict[tuple, dict] = {}
    for light in lights:
        by_key[light_key(light)] = light
    return [by_key[k] for k in sorted(by_key.keys())]


def enforce_ramp_floor_rules(map_data: dict) -> None:
    # Mutates `map_data` in place. Only called from `canonicalize_map` after a
    # `deepcopy`, so the mutation is safe.
    for ramp in map_data["ramps"]:
        lower = ramp["lower_level"]
        upper = lower + 1
        if lower < 0 or upper >= len(map_data["levels"]):
            continue
        cells = ramp_cells(ramp)
        if not cells:
            continue

        # Ensure the ramp's footprint cells exist as regular floors on the
        # lower level (auto-painted with placeholder materials when missing),
        # and are removed from the upper level. Inaccessible-floor entries
        # at those cells are also dropped.
        ramp_faces = {face: ramp.get(face, "") for face in FACES}
        lower_existing = {(f["col"], f["row"]): f for f in map_data["levels"][lower]["floors"]}
        for col, row in cells:
            if (col, row) not in lower_existing:
                lower_existing[(col, row)] = {"col": col, "row": row, **ramp_faces}
        map_data["levels"][lower]["floors"] = list(lower_existing.values())
        map_data["levels"][lower]["inaccessible_floors"] = [
            f for f in map_data["levels"][lower]["inaccessible_floors"] if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["floors"] = [
            f for f in map_data["levels"][upper]["floors"] if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["inaccessible_floors"] = [
            f for f in map_data["levels"][upper]["inaccessible_floors"] if (f["col"], f["row"]) not in cells
        ]

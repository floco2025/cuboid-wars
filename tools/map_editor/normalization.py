"""Map-data normalization, canonical ordering, dedupe, and resize."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    BARRIER_KIND_TABLE,
    DEFAULT_ALIAS,
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    FACES,
    ITEM_KEY_TYPE,
    LADDER_SIDES,
    LIGHT_SIDES,
)
from .display import expand_face_materials
from .geometry import normalized_wall, ramp_cells, wall_endpoints_for_cell_side

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
                "lights": [normalize_light(l) for l in level.get("lights", [])],
            }
        )
    if not levels:
        levels = [
            {
                "name": "Level 0",
                "floors": [],
                "inaccessible_floors": [],
                "grass": [],
                "walls": [],
                "barriers": [],
                "lights": [],
            }
        ]

    ramps = [normalize_ramp(r) for r in map_data.get("ramps", [])]
    ladders = [normalize_ladder(l) for l in map_data.get("ladders", [])]
    return {
        "grid_cols": cols,
        "grid_rows": rows,
        "actor_spawn_zones": actor_spawn_zones,
        "player_spawn_zones": player_spawn_zones,
        "items": items,
        "pressure_plates": pressure_plates,
        "levels": levels,
        "ramps": ramps,
        "ladders": ladders,
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


def normalize_barrier(barrier: dict) -> dict:
    # Migration: maps from before the kind-table refactor used `color`. Accept
    # either field name on read; serialize as `kind` going forward.
    raw = barrier.get("kind", barrier.get("color", ""))
    kind = str(raw)
    if BARRIER_KIND_TABLE and kind not in BARRIER_KIND_TABLE:
        # Unknown id stays as-is so `validate_map` can surface it; falling
        # back silently would hide authoring errors.
        kind = kind
    return {
        "c0": int(barrier["c0"]),
        "r0": int(barrier["r0"]),
        "c1": int(barrier["c1"]),
        "r1": int(barrier["r1"]),
        "kind": kind,
    }


def normalize_ramp(ramp: dict) -> dict:
    return {
        "low": [int(ramp["low"][0]), int(ramp["low"][1])],
        "high": [int(ramp["high"][0]), int(ramp["high"][1])],
        "lower_level": int(ramp["lower_level"]),
        **expand_face_materials(ramp),
    }


def normalize_ladder(ladder: dict) -> dict:
    side = str(ladder.get("side", "")).upper()
    return {
        "lower_level": int(ladder.get("lower_level", 0)),
        "col": int(ladder["col"]),
        "row": int(ladder["row"]),
        "side": side if side in LADDER_SIDES else "N",
        "levels": max(1, int(ladder.get("levels", 1))),
    }


def ladder_key(ladder: dict) -> tuple:
    return (ladder["lower_level"], ladder["row"], ladder["col"], ladder["side"], ladder["levels"])


def ladder_edge_key(ladder: dict) -> tuple:
    return (ladder["row"], ladder["col"], ladder["side"])


def ladder_spans_level(ladder: dict, level_idx: int) -> bool:
    return ladder["lower_level"] <= level_idx <= ladder["lower_level"] + ladder["levels"]


def normalize_light(light: dict) -> dict:
    side = str(light.get("side", "")).upper()
    return {
        "col": int(light["col"]),
        "row": int(light["row"]),
        "side": side if side in LIGHT_SIDES else "N",
    }


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
    return {**_normalize_zone_rect(zone), "kind": kind, "count": max(0, count)}


def normalize_player_spawn_zone(zone: dict) -> dict:
    return _normalize_zone_rect(zone)


def normalize_item(item: dict) -> dict:
    out = {
        "level": int(item.get("level", 0)),
        "col": int(item.get("col", 0)),
        "row": int(item.get("row", 0)),
        "type": str(item.get("type", "")),
    }
    # Only keys carry a kind; a stray kind on another type is dropped here
    # so it can't survive into the saved file (the Rust loader rejects it).
    if out["type"] == ITEM_KEY_TYPE:
        out["kind"] = str(item.get("kind", ""))
    return out


def item_key(item: dict) -> tuple:
    return (item["level"], item["row"], item["col"])


def normalize_pressure_plate(plate: dict) -> dict:
    return {
        "level": int(plate.get("level", 0)),
        "col": int(plate.get("col", 0)),
        "row": int(plate.get("row", 0)),
        "kind": str(plate.get("kind", "")),
    }


def pressure_plate_key(plate: dict) -> tuple:
    return (plate["level"], plate["row"], plate["col"], plate["kind"])


def actor_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
        zone["kind"],
        zone["count"],
    )


def player_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
    )


def zone_key(list_name: str, zone: dict) -> tuple:
    if list_name == ACTOR_ZONE_LIST:
        return actor_zone_key(zone)
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
    enforce_ramp_floor_rules(b)
    b["actor_spawn_zones"] = _dedupe_sorted(b["actor_spawn_zones"], actor_zone_key)
    b["player_spawn_zones"] = _dedupe_sorted(b["player_spawn_zones"], player_zone_key)
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
            f for f in _dedupe_floors(level["inaccessible_floors"])
            if (f["col"], f["row"]) not in floor_keys
        ]
        ramp_set = ramp_cells_by_level[level_idx]
        # Grass only survives on slab cells (floor or inaccessible floor)
        # outside ramp footprints, so erasing a floor drops its grass in the
        # same canonicalize pass — the two can never desync.
        slab_keys = floor_keys | {(f["col"], f["row"]) for f in level["inaccessible_floors"]}
        level["grass"] = [
            g for g in _dedupe_floors(level["grass"])
            if (g["col"], g["row"]) in slab_keys and (g["col"], g["row"]) not in ramp_set
        ]
        level["walls"] = _dedupe_walls(level["walls"])
        wall_endpoints_set = {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]]))
            for w in level["walls"]
        }
        # Drop barriers that share an edge with a wall on the same level so
        # canonical files satisfy the Rust loader's conflict rule.
        level["barriers"] = [
            b for b in _dedupe_barriers(level.get("barriers", []))
            if tuple(normalized_wall([b["c0"], b["r0"], b["c1"], b["r1"]])) not in wall_endpoints_set
        ]
        cols, rows = b["grid_cols"], b["grid_rows"]
        in_bounds_lights = [
            l for l in level.get("lights", [])
            if 0 <= l["col"] < cols and 0 <= l["row"] < rows
            and l["side"] in LIGHT_SIDES
            and wall_endpoints_for_cell_side(l["col"], l["row"], l["side"]) in wall_endpoints_set
            and (l["col"], l["row"]) not in ramp_set
        ]
        level["lights"] = _dedupe_lights(in_bounds_lights)

    # Items only survive on regular-floor cells outside lower-level ramp
    # footprints — the server's `has_floor && !has_ramp` rule — so erasing a
    # floor (or laying a ramp) drops its item in the same canonicalize pass.
    lower_ramp_cells_by_level: list[set[tuple[int, int]]] = [set() for _ in b["levels"]]
    for ramp in b["ramps"]:
        lower = ramp["lower_level"]
        if 0 <= lower < len(lower_ramp_cells_by_level):
            lower_ramp_cells_by_level[lower].update(ramp_cells(ramp))
    floor_keys_by_level = [{(f["col"], f["row"]) for f in level["floors"]} for level in b["levels"]]
    items_by_cell: dict[tuple[int, int, int], dict] = {}
    for item in b["items"]:
        level_idx = item["level"]
        if not (0 <= level_idx < len(b["levels"])):
            continue
        cell_key = (item["col"], item["row"])
        if cell_key not in floor_keys_by_level[level_idx] or cell_key in lower_ramp_cells_by_level[level_idx]:
            continue
        # Later entries win when the same cell holds two items, so the
        # user's most recent placement stays.
        items_by_cell[(level_idx, item["col"], item["row"])] = item
    b["items"] = [items_by_cell[k] for k in sorted(items_by_cell.keys(), key=lambda k: (k[0], k[2], k[1]))]

    b["ramps"] = sorted(
        b["ramps"],
        key=lambda r: (r["lower_level"], tuple(r["low"]), tuple(r["high"])),
    )

    # Ladders: drop out-of-bounds anchors and spans past the top level (both
    # hard errors in the Rust loader), then keep the first ladder per
    # overlapping same-edge span — the loader rejects overlaps.
    cols, rows = b["grid_cols"], b["grid_rows"]
    in_bounds_ladders = [
        l for l in b["ladders"]
        if 0 <= l["col"] < cols and 0 <= l["row"] < rows
        and l["lower_level"] >= 0
        and l["lower_level"] + l["levels"] < len(b["levels"])
    ]
    kept_ladders: list[dict] = []
    for ladder in sorted(in_bounds_ladders, key=ladder_key):
        overlapping = any(
            ladder_edge_key(other) == ladder_edge_key(ladder)
            and other["lower_level"] < ladder["lower_level"] + ladder["levels"]
            and ladder["lower_level"] < other["lower_level"] + other["levels"]
            for other in kept_ladders
        )
        if not overlapping:
            kept_ladders.append(ladder)
    b["ladders"] = kept_ladders
    return b


def _dedupe_floors(floors: list[dict]) -> list[dict]:
    by_pos: dict[tuple[int, int], dict] = {}
    for floor in floors:
        by_pos[(floor["col"], floor["row"])] = floor
    return [by_pos[k] for k in sorted(by_pos.keys(), key=lambda p: (p[1], p[0]))]


def _dedupe_walls(walls: list[dict]) -> list[dict]:
    by_edge: dict[tuple[int, int, int, int], dict] = {}
    for wall in walls:
        c0, r0, c1, r1 = normalized_wall([wall["c0"], wall["r0"], wall["c1"], wall["r1"]])
        by_edge[(c0, r0, c1, r1)] = {**wall, "c0": c0, "r0": r0, "c1": c1, "r1": r1}
    return [by_edge[k] for k in sorted(by_edge.keys())]


def _dedupe_barriers(barriers: list[dict]) -> list[dict]:
    by_edge: dict[tuple[int, int, int, int], dict] = {}
    for barrier in barriers:
        c0, r0, c1, r1 = normalized_wall([barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]])
        by_edge[(c0, r0, c1, r1)] = {**barrier, "c0": c0, "r0": r0, "c1": c1, "r1": r1}
    return [by_edge[k] for k in sorted(by_edge.keys())]


def _dedupe_lights(lights: list[dict]) -> list[dict]:
    by_key: dict[tuple, dict] = {}
    for light in lights:
        by_key[light_key(light)] = light
    return [by_key[k] for k in sorted(by_key.keys())]


def resize_map_data(
    map_data: dict, new_cols: int, new_rows: int, anchor_x: int, anchor_y: int
) -> dict:
    """Translate and clip every coordinate in a map to fit a new grid size.

    `anchor_x` and `anchor_y` are each one of {0, 1, 2} indicating where the
    old grid sits inside the new grid (0=left/top, 1=center, 2=right/bottom).
    Cells, wall endpoints, and ramp endpoints are translated by the resulting
    offset; anything that falls outside the new bounds is dropped. Materials
    ride along with each segment.
    """
    old_cols = map_data["grid_cols"]
    old_rows = map_data["grid_rows"]
    dc = (new_cols - old_cols) * anchor_x // 2
    dr = (new_rows - old_rows) * anchor_y // 2

    out = copy.deepcopy(map_data)
    out["grid_cols"] = new_cols
    out["grid_rows"] = new_rows

    def cell_in_bounds(c: int, r: int) -> bool:
        return 0 <= c < new_cols and 0 <= r < new_rows

    def line_in_bounds(c: int, r: int) -> bool:
        return 0 <= c <= new_cols and 0 <= r <= new_rows

    def shift_floor(f: dict) -> dict | None:
        nc, nr = f["col"] + dc, f["row"] + dr
        if not cell_in_bounds(nc, nr):
            return None
        return {**f, "col": nc, "row": nr}

    def shift_wall(w: dict) -> dict | None:
        nc0, nr0 = w["c0"] + dc, w["r0"] + dr
        nc1, nr1 = w["c1"] + dc, w["r1"] + dr
        if not (line_in_bounds(nc0, nr0) and line_in_bounds(nc1, nr1)):
            return None
        return {**w, "c0": nc0, "r0": nr0, "c1": nc1, "r1": nr1}

    def shift_light(light: dict) -> dict | None:
        nc, nr = light["col"] + dc, light["row"] + dr
        if not cell_in_bounds(nc, nr):
            return None
        return {**light, "col": nc, "row": nr}

    for level in out["levels"]:
        level["floors"] = [f for f in (shift_floor(f) for f in level["floors"]) if f is not None]
        level["inaccessible_floors"] = [
            f for f in (shift_floor(f) for f in level["inaccessible_floors"]) if f is not None
        ]
        level["grass"] = [
            g for g in (shift_floor(g) for g in level.get("grass", [])) if g is not None
        ]
        level["walls"] = [w for w in (shift_wall(w) for w in level["walls"]) if w is not None]
        level["barriers"] = [
            b for b in (shift_wall(b) for b in level.get("barriers", [])) if b is not None
        ]
        level["lights"] = [
            l for l in (shift_light(l) for l in level.get("lights", [])) if l is not None
        ]

    def clip_zone(zone: dict) -> dict | None:
        c0, c1 = zone["cols"]
        r0, r1 = zone["rows"]
        nc0 = max(0, c0 + dc)
        nc1 = min(new_cols, c1 + dc)
        nr0 = max(0, r0 + dr)
        nr1 = min(new_rows, r1 + dr)
        if nc1 <= nc0 or nr1 <= nr0:
            return None
        zone["cols"] = [nc0, nc1]
        zone["rows"] = [nr0, nr1]
        return zone

    out["actor_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["actor_spawn_zones"]) if z is not None
    ]
    out["player_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["player_spawn_zones"]) if z is not None
    ]

    def clip_cell_entry(entry: dict) -> dict | None:
        nc = entry["col"] + dc
        nr = entry["row"] + dr
        if not (0 <= nc < new_cols and 0 <= nr < new_rows):
            return None
        entry["col"] = nc
        entry["row"] = nr
        return entry

    out["pressure_plates"] = [
        p for p in (clip_cell_entry(p) for p in out.get("pressure_plates", [])) if p is not None
    ]
    out["items"] = [i for i in (clip_cell_entry(i) for i in out.get("items", [])) if i is not None]

    kept_ramps = []
    for ramp in out["ramps"]:
        low_c, low_r = ramp["low"][0] + dc, ramp["low"][1] + dr
        high_c, high_r = ramp["high"][0] + dc, ramp["high"][1] + dr
        if line_in_bounds(low_c, low_r) and line_in_bounds(high_c, high_r):
            ramp["low"] = [low_c, low_r]
            ramp["high"] = [high_c, high_r]
            kept_ramps.append(ramp)
    out["ramps"] = kept_ramps

    out["ladders"] = [
        l for l in (clip_cell_entry(l) for l in out.get("ladders", [])) if l is not None
    ]

    return out


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
        # Placeholder is an alias (face values must be aliases — see
        # `validate_map`); the user can re-paint with the right material
        # later. Source from the loaded catalog rather than hard-coding so
        # the value can't drift to a removed alias.
        ramp_faces = {face: ramp.get(face, DEFAULT_ALIAS) for face in FACES}
        lower_existing = {(f["col"], f["row"]): f for f in map_data["levels"][lower]["floors"]}
        for col, row in cells:
            if (col, row) not in lower_existing:
                lower_existing[(col, row)] = {"col": col, "row": row, **ramp_faces}
        map_data["levels"][lower]["floors"] = list(lower_existing.values())
        map_data["levels"][lower]["inaccessible_floors"] = [
            f for f in map_data["levels"][lower]["inaccessible_floors"]
            if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["floors"] = [
            f for f in map_data["levels"][upper]["floors"]
            if (f["col"], f["row"]) not in cells
        ]
        map_data["levels"][upper]["inaccessible_floors"] = [
            f for f in map_data["levels"][upper]["inaccessible_floors"]
            if (f["col"], f["row"]) not in cells
        ]



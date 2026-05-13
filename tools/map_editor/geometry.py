"""Map normalization, canonicalization, and editor geometry helpers."""

from __future__ import annotations

import copy
import hashlib

from PySide6.QtCore import QPoint
from PySide6.QtGui import QColor

from .constants import (
    ACTOR_ZONE_LIST,
    BARRIER_KIND_TABLE,
    COOKIE_ZONE_LIST,
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    FACES,
    KEY_ZONE_LIST,
    LIGHT_SIDES,
    MODE_COOKIE_SPAWN_PAINT,
    MODE_ERASE,
    MODE_ERASE_KEEP_FLOORS,
    MODE_ERASE_LIGHTS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_INACCESSIBLE_FLOOR,
    MODE_KEY_SPAWN_PAINT,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_RAMP_MATERIAL,
    PLAYER_ZONE_LIST,
)


def normalize_map(map_data: dict) -> dict:
    cols = int(map_data.get("grid_cols", DEFAULT_GRID_COLS))
    rows = int(map_data.get("grid_rows", DEFAULT_GRID_ROWS))
    actor_spawn_zones = [normalize_actor_spawn_zone(z) for z in map_data.get("actor_spawn_zones", [])]
    player_spawn_zones = [normalize_player_spawn_zone(z) for z in map_data.get("player_spawn_zones", [])]
    cookie_spawn_zones = [normalize_cookie_spawn_zone(z) for z in map_data.get("cookie_spawn_zones", [])]
    key_spawn_zones = [normalize_key_spawn_zone(z) for z in map_data.get("key_spawn_zones", [])]
    levels = []
    for idx, level in enumerate(map_data.get("levels", [])):
        levels.append(
            {
                "name": str(level.get("name") or f"Level {idx}"),
                "floors": [normalize_floor(f) for f in level.get("floors", [])],
                "inaccessible_floors": [normalize_floor(f) for f in level.get("inaccessible_floors", [])],
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
                "walls": [],
                "barriers": [],
                "lights": [],
            }
        ]

    ramps = [normalize_ramp(r) for r in map_data.get("ramps", [])]
    return {
        "grid_cols": cols,
        "grid_rows": rows,
        "actor_spawn_zones": actor_spawn_zones,
        "player_spawn_zones": player_spawn_zones,
        "cookie_spawn_zones": cookie_spawn_zones,
        "key_spawn_zones": key_spawn_zones,
        "levels": levels,
        "ramps": ramps,
    }


def normalize_floor(floor: dict) -> dict:
    return {
        "col": int(floor["col"]),
        "row": int(floor["row"]),
        **expand_face_materials(floor),
    }


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


def normalize_light(light: dict) -> dict:
    side = str(light.get("side", "")).upper()
    return {
        "col": int(light["col"]),
        "row": int(light["row"]),
        "side": side if side in LIGHT_SIDES else "N",
    }


def light_key(light: dict) -> tuple:
    return (light["row"], light["col"], light["side"])


def wall_endpoints_for_cell_side(col: int, row: int, side: str) -> tuple[int, int, int, int]:
    """Return the canonical (c0, r0, c1, r1) of the wall on a cell's side."""
    if side == "N":
        c0, r0, c1, r1 = col, row, col + 1, row
    elif side == "S":
        c0, r0, c1, r1 = col, row + 1, col + 1, row + 1
    elif side == "W":
        c0, r0, c1, r1 = col, row, col, row + 1
    elif side == "E":
        c0, r0, c1, r1 = col + 1, row, col + 1, row + 1
    else:
        raise ValueError(f"unknown side {side!r}")
    return tuple(normalized_wall([c0, r0, c1, r1]))


def cell_side_from_click(col: int, row: int, px: float, py: float) -> str:
    """Return the cardinal side of cell (col, row) that the click (px, py) is
    closest to, in cell-unit coordinates."""
    distances = {
        "N": py - row,
        "S": (row + 1) - py,
        "W": px - col,
        "E": (col + 1) - px,
    }
    return min(distances, key=distances.get)


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


def normalize_cookie_spawn_zone(zone: dict) -> dict:
    return _normalize_zone_rect(zone)


def normalize_key_spawn_zone(zone: dict) -> dict:
    return {**_normalize_zone_rect(zone), "kind": str(zone.get("kind", ""))}


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


def cookie_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
    )


def key_zone_key(zone: dict) -> tuple:
    return (
        zone["level"],
        zone["rows"][0],
        zone["cols"][0],
        zone["rows"][1],
        zone["cols"][1],
        zone["kind"],
    )


def zone_key(list_name: str, zone: dict) -> tuple:
    if list_name == ACTOR_ZONE_LIST:
        return actor_zone_key(zone)
    if list_name == COOKIE_ZONE_LIST:
        return cookie_zone_key(zone)
    if list_name == KEY_ZONE_LIST:
        return key_zone_key(zone)
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
    b["cookie_spawn_zones"] = _dedupe_sorted(b["cookie_spawn_zones"], cookie_zone_key)
    b["key_spawn_zones"] = _dedupe_sorted(b["key_spawn_zones"], key_zone_key)
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
        ramp_set = ramp_cells_by_level[level_idx]
        in_bounds_lights = [
            l for l in level.get("lights", [])
            if 0 <= l["col"] < cols and 0 <= l["row"] < rows
            and l["side"] in LIGHT_SIDES
            and wall_endpoints_for_cell_side(l["col"], l["row"], l["side"]) in wall_endpoints_set
            and (l["col"], l["row"]) not in ramp_set
        ]
        level["lights"] = _dedupe_lights(in_bounds_lights)

    b["ramps"] = sorted(
        b["ramps"],
        key=lambda r: (r["lower_level"], tuple(r["low"]), tuple(r["high"])),
    )
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
    out["cookie_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["cookie_spawn_zones"]) if z is not None
    ]
    out["key_spawn_zones"] = [
        z for z in (clip_zone(z) for z in out["key_spawn_zones"]) if z is not None
    ]

    kept_ramps = []
    for ramp in out["ramps"]:
        low_c, low_r = ramp["low"][0] + dc, ramp["low"][1] + dr
        high_c, high_r = ramp["high"][0] + dc, ramp["high"][1] + dr
        if line_in_bounds(low_c, low_r) and line_in_bounds(high_c, high_r):
            ramp["low"] = [low_c, low_r]
            ramp["high"] = [high_c, high_r]
            kept_ramps.append(ramp)
    out["ramps"] = kept_ramps

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
        # `validate_map`); the user can re-paint with the right material later.
        ramp_faces = {face: ramp.get(face, "slab") for face in FACES}
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


def normalized_wall(wall: list[int]) -> list[int]:
    c0, r0, c1, r1 = wall
    if (c1, r1) < (c0, r0):
        return [c1, r1, c0, r0]
    return [c0, r0, c1, r1]



def zone_cells(zone: dict) -> list[tuple[int, int]]:
    c0, c1 = zone["cols"]
    r0, r1 = zone["rows"]
    return [(c, r) for r in range(r0, r1) for c in range(c0, c1)]


def zone_rect(zone: dict) -> tuple[int, int, int, int]:
    return zone["cols"][0], zone["rows"][0], zone["cols"][1], zone["rows"][1]


def zone_intersects_rect(zone: dict, rect: tuple[int, int, int, int]) -> bool:
    return rects_overlap(zone_rect(zone), rect)


def zone_contains_cell(zone: dict, col: int, row: int) -> bool:
    c0, r0, c1, r1 = zone_rect(zone)
    return c0 <= col < c1 and r0 <= row < r1


def zone_color(kind: str) -> QColor:
    if not kind:
        return QColor(34, 197, 94)
    return tag_color(kind)


def tag_color(tag: str) -> QColor:
    digest = hashlib.md5(tag.encode("utf-8")).digest()
    hue = (digest[0] | (digest[1] << 8)) % 360
    color = QColor()
    color.setHsv(hue, 165, 220)
    return color


WALL_PEN_WIDTH = 6
WALL_HIGHLIGHT_WIDTH = WALL_PEN_WIDTH + 4
# Barriers render slightly thinner than walls so the two read as distinct
# even when their colors happen to be close.
BARRIER_PEN_WIDTH = 4

# Translucent rectangle preview drawn while dragging in modes that operate on
# a cell rectangle. Lookup falls back to a neutral green for any mode that
# uses the rect-preview UI but isn't listed here (e.g. actor spawn paint).
DRAG_PREVIEW_FALLBACK = QColor(34, 197, 94, 120)
DRAG_PREVIEW_COLORS: dict[str, QColor] = {
    MODE_FLOOR: QColor(111, 180, 255, 120),
    MODE_INACCESSIBLE_FLOOR: QColor(148, 163, 184, 120),
    MODE_PLAYER_SPAWN_PAINT: QColor(99, 102, 241, 120),
    MODE_COOKIE_SPAWN_PAINT: QColor(250, 204, 21, 120),  # gold — matches cookie material
    # Kind is picked *after* the drag, so the preview color is a neutral
    # off-white. The placed rect is then color-coded by its kind.
    MODE_KEY_SPAWN_PAINT: QColor(220, 220, 220, 110),
    MODE_FLOOR_MATERIAL: QColor(236, 72, 153, 120),
    MODE_RAMP_MATERIAL: QColor(168, 85, 247, 120),  # purple to distinguish from floor mode pink
    MODE_ERASE: QColor(248, 113, 113, 120),
    MODE_ERASE_KEEP_FLOORS: QColor(251, 146, 60, 120),
    MODE_ERASE_LIGHTS: QColor(250, 204, 21, 120),   # amber — distinct from red erase tools
}


def face_color(seg: dict) -> QColor:
    """Color derived from the segment's full six-face material composition.
    Two segments (floor / wall / ramp) sharing the same six face values get
    the same color; differing on any face produces a different one. Saturation
    is pinned to max so hue differences read clearly; value also varies a bit
    so close hues remain distinguishable."""
    digest = hashlib.md5("|".join(seg.get(face, "") for face in FACES).encode("utf-8")).digest()
    hue = int.from_bytes(digest[:2], "big") % 360
    value = 200 + (digest[2] % 56)  # 200-255
    color = QColor()
    color.setHsv(hue, 255, value)
    return color


def expand_face_materials(obj: dict) -> dict[str, str]:
    """Expand `all` shorthand into six explicit face materials. Faces not
    explicitly set fall back to `all` (or to any other face value if `all` is
    absent). Used when reading per-segment material data from JSON."""
    fallback = obj.get("all")
    if fallback is None:
        fallback = next((obj[face] for face in FACES if face in obj), None)
    if fallback is None:
        # No materials at all on this segment — fall back to a sentinel so the
        # editor can still display something. Real maps shouldn't hit this.
        fallback = "fiberous-plaster1-ue"
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


def materials_summary(seg: dict) -> str:
    """One-line summary of a segment's six face materials, using the same
    `all`/overrides compaction as the on-disk shape."""
    compact = compact_face_materials(seg)
    return ", ".join(f"{k}={v}" for k, v in compact.items())


def level_label(level: dict, index: int) -> str:
    name = level.get("name")
    return f"Level {index}" if not name else f"Level {index} ({name})"


def grid_point_in_bounds(col: int, row: int, cols: int, rows: int) -> bool:
    return 0 <= col <= cols and 0 <= row <= rows


def ramp_error(low: list[int], high: list[int], lower_level: int, cols: int, rows: int, level_count: int) -> str | None:
    if lower_level < 0 or lower_level + 1 >= level_count:
        return "lower_level must have an upper level"
    if not grid_point_in_bounds(low[0], low[1], cols, rows):
        return "low point is outside the grid-line bounds"
    if not grid_point_in_bounds(high[0], high[1], cols, rows):
        return "high point is outside the grid-line bounds"
    width = abs(high[0] - low[0])
    height = abs(high[1] - low[1])
    if width == 0 or height == 0:
        return "ramp must span a non-empty rectangular footprint"
    if width == height:
        return "ramp needs one clear longer axis"
    return None


def ramp_rect(ramp: dict) -> tuple[int, int, int, int]:
    low = ramp["low"]
    high = ramp["high"]
    return min(low[0], high[0]), min(low[1], high[1]), max(low[0], high[0]), max(low[1], high[1])


def ramp_cells(ramp: dict) -> set[tuple[int, int]]:
    c0, r0, c1, r1 = ramp_rect(ramp)
    return {(col, row) for row in range(r0, r1) for col in range(c0, c1)}


def ramp_axis(ramp: dict) -> str:
    low = ramp["low"]
    high = ramp["high"]
    dx = high[0] - low[0]
    dy = high[1] - low[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def opposite_direction(direction: str) -> str:
    return {
        "north": "south",
        "south": "north",
        "east": "west",
        "west": "east",
    }[direction]


# ============================================================================
# Drag / paint geometry helpers (cell rects, wall edges, ramp shapes)
# ============================================================================


def rect_from_cells(a: tuple[int, int], b: tuple[int, int]) -> tuple[int, int, int, int]:
    c0 = min(a[0], b[0])
    r0 = min(a[1], b[1])
    c1 = max(a[0], b[0]) + 1
    r1 = max(a[1], b[1]) + 1
    return c0, r0, c1, r1


def ramp_points_from_cells(start: tuple[int, int], end: tuple[int, int]) -> tuple[list[int], list[int]]:
    c0, r0, c1, r1 = rect_from_cells(start, end)
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) >= abs(dy):
        if dx >= 0:
            return [c0, r0], [c1, r1]
        return [c1, r0], [c0, r1]
    if dy >= 0:
        return [c0, r0], [c1, r1]
    return [c0, r1], [c1, r0]


def rects_overlap(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> bool:
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def wall_overlaps_rect(wall: list[int], rect: tuple[int, int, int, int]) -> bool:
    c0, r0, c1, r1 = rect
    wc0, wr0, wc1, wr1 = wall
    if wr0 == wr1:
        left = min(wc0, wc1)
        right = max(wc0, wc1)
        return r0 <= wr0 <= r1 and left < c1 and c0 < right
    top = min(wr0, wr1)
    bottom = max(wr0, wr1)
    return c0 <= wc0 <= c1 and top < r1 and r0 < bottom


def snapped_wall_end(start: tuple[int, int], current: tuple[int, int]) -> tuple[int, int]:
    dx = current[0] - start[0]
    dy = current[1] - start[1]
    if abs(dx) >= abs(dy):
        return current[0], start[1]
    return start[0], current[1]


def draw_direction(start: tuple[int, int], end: tuple[int, int]) -> str:
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def orthogonal_arrow_points(
    c0: int,
    r0: int,
    c1: int,
    r1: int,
    direction: str,
    cell: float,
) -> tuple[tuple[float, float], tuple[float, float]]:
    pad = min(cell * 0.35, 14.0)
    left = c0 * cell + pad
    right = c1 * cell - pad
    top = r0 * cell + pad
    bottom = r1 * cell - pad
    mid_x = (c0 + c1) * cell / 2.0
    mid_y = (r0 + r1) * cell / 2.0
    if direction == "east":
        return (left, mid_y), (right, mid_y)
    if direction == "west":
        return (right, mid_y), (left, mid_y)
    if direction == "south":
        return (mid_x, top), (mid_x, bottom)
    return (mid_x, bottom), (mid_x, top)


def wall_segments_between(start: tuple[int, int], end: tuple[int, int]) -> list[list[int]]:
    if start == end:
        return []
    c0, r0 = start
    c1, r1 = end
    edges = []
    if r0 == r1:
        step = 1 if c1 > c0 else -1
        for col in range(c0, c1, step):
            edges.append(normalized_wall([col, r0, col + step, r0]))
    elif c0 == c1:
        step = 1 if r1 > r0 else -1
        for row in range(r0, r1, step):
            edges.append(normalized_wall([c0, row, c0, row + step]))
    return edges


_LIGHT_MARKER_BASE = 0.08   # cells: distance from the wall to the marker's base
_LIGHT_MARKER_TIP = 0.30    # cells: distance from the wall to the marker's tip
_LIGHT_MARKER_HALF_W = 0.12 # cells: half-width of the marker's base


def light_marker_polygon(light: dict, cell: float) -> list[QPoint]:
    """Filled triangle marker, anchored at the wall midpoint, pointing into
    the room from the cell side the light sits on."""
    col, row, side = light["col"], light["row"], light["side"]
    base = _LIGHT_MARKER_BASE
    tip = _LIGHT_MARKER_TIP
    half = _LIGHT_MARKER_HALF_W
    if side == "N":
        pts = [(0.5, tip), (0.5 - half, base), (0.5 + half, base)]
    elif side == "S":
        pts = [(0.5, 1 - tip), (0.5 - half, 1 - base), (0.5 + half, 1 - base)]
    elif side == "W":
        pts = [(tip, 0.5), (base, 0.5 - half), (base, 0.5 + half)]
    else:  # "E"
        pts = [(1 - tip, 0.5), (1 - base, 0.5 - half), (1 - base, 0.5 + half)]
    return [QPoint(round((col + dx) * cell), round((row + dy) * cell)) for dx, dy in pts]


def point_near_wall(px: float, py: float, wall: list[int], tolerance: float = 0.16) -> bool:
    c0, r0, c1, r1 = wall
    if r0 == r1:
        return min(c0, c1) - tolerance <= px <= max(c0, c1) + tolerance and abs(py - r0) <= tolerance
    return min(r0, r1) - tolerance <= py <= max(r0, r1) + tolerance and abs(px - c0) <= tolerance

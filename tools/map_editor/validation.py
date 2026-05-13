"""Structural validation for map data."""

from __future__ import annotations

from .constants import BARRIER_KIND_TABLE, FACES, LIGHT_SIDES, MATERIAL_ALIASES
from .geometry import (
    grid_point_in_bounds,
    level_label,
    normalized_wall,
    ramp_error,
    wall_endpoints_for_cell_side,
)


def validate_map(map_data: dict) -> list[str]:
    errors: list[str] = []
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    if cols <= 0 or rows <= 0:
        errors.append("grid_cols and grid_rows must be positive")
    if not map_data["levels"]:
        errors.append("at least one level is required")
    if not map_data["player_spawn_zones"]:
        errors.append("at least one player_spawn_zones entry is required by the Rust loader")

    for idx, zone in enumerate(map_data["actor_spawn_zones"]):
        _validate_zone_rect(zone, f"actor_spawn_zones[{idx}]", map_data, errors)
        if not zone["kind"]:
            errors.append(f"actor_spawn_zones[{idx}] has empty `kind`")
        if zone["count"] < 0:
            errors.append(f"actor_spawn_zones[{idx}] has negative count")

    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _validate_zone_rect(zone, f"player_spawn_zones[{idx}]", map_data, errors)

    for idx, zone in enumerate(map_data["cookie_spawn_zones"]):
        _validate_zone_rect(zone, f"cookie_spawn_zones[{idx}]", map_data, errors)

    for idx, zone in enumerate(map_data["key_spawn_zones"]):
        _validate_zone_rect(zone, f"key_spawn_zones[{idx}]", map_data, errors)
        kind = zone.get("kind")
        if kind not in BARRIER_KIND_TABLE:
            known = ", ".join(BARRIER_KIND_TABLE) or "(none configured)"
            errors.append(f"key_spawn_zones[{idx}] has unknown kind {kind!r}; known: [{known}]")

    for level_idx, level in enumerate(map_data["levels"]):
        prefix = level_label(level, level_idx)
        if not level["floors"]:
            errors.append(f"{prefix}: at least one floor is required by the Rust loader")
        floor_set = {(f["col"], f["row"]) for f in level["floors"]}
        for floor in level["floors"]:
            c, r = floor["col"], floor["row"]
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: floor [{c}, {r}] is outside the grid")
        for floor in level["inaccessible_floors"]:
            c, r = floor["col"], floor["row"]
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: inaccessible floor [{c}, {r}] is outside the grid")
            if (c, r) in floor_set:
                errors.append(f"{prefix}: inaccessible floor [{c}, {r}] overlaps a floor")
        for wall in level["walls"]:
            c0, r0, c1, r1 = wall["c0"], wall["r0"], wall["c1"], wall["r1"]
            if not (grid_point_in_bounds(c0, r0, cols, rows) and grid_point_in_bounds(c1, r1, cols, rows)):
                errors.append(f"{prefix}: wall [{c0}, {r0}, {c1}, {r1}] is outside the grid-line bounds")
            if abs(c1 - c0) + abs(r1 - r0) != 1:
                errors.append(f"{prefix}: wall [{c0}, {r0}, {c1}, {r1}] is not one grid edge")

        wall_endpoints_set = {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]]))
            for w in level["walls"]
        }
        barrier_seen: set[tuple[int, int, int, int]] = set()
        for idx, barrier in enumerate(level.get("barriers", [])):
            c0, r0, c1, r1 = barrier["c0"], barrier["r0"], barrier["c1"], barrier["r1"]
            kind = barrier.get("kind")
            if not (grid_point_in_bounds(c0, r0, cols, rows) and grid_point_in_bounds(c1, r1, cols, rows)):
                errors.append(f"{prefix}: barrier[{idx}] [{c0}, {r0}, {c1}, {r1}] is outside the grid-line bounds")
            if abs(c1 - c0) + abs(r1 - r0) != 1:
                errors.append(f"{prefix}: barrier[{idx}] [{c0}, {r0}, {c1}, {r1}] is not one grid edge")
            if kind not in BARRIER_KIND_TABLE:
                known = ", ".join(BARRIER_KIND_TABLE) or "(none configured)"
                errors.append(f"{prefix}: barrier[{idx}] has unknown kind {kind!r}; known: [{known}]")
            key = tuple(normalized_wall([c0, r0, c1, r1]))
            if key in wall_endpoints_set:
                errors.append(f"{prefix}: barrier[{idx}] {list(key)} overlaps a wall")
            if key in barrier_seen:
                errors.append(f"{prefix}: barrier[{idx}] {list(key)} duplicates another barrier")
            barrier_seen.add(key)

        for light in level.get("lights", []):
            c, r, side = light["col"], light["row"], light["side"]
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{prefix}: light [{c}, {r}, {side}] is outside the grid")
                continue
            if side not in LIGHT_SIDES:
                errors.append(f"{prefix}: light [{c}, {r}, {side}] has invalid side")
                continue
            if wall_endpoints_for_cell_side(c, r, side) not in wall_endpoints_set:
                errors.append(f"{prefix}: light [{c}, {r}, {side}] has no wall on that side")

    for ramp in map_data["ramps"]:
        msg = ramp_error(ramp["low"], ramp["high"], ramp["lower_level"], cols, rows, len(map_data["levels"]))
        if msg:
            errors.append(f"ramp {ramp}: {msg}")

    # Face values on walls, floors, ramps must be aliases (assets.json::aliases).
    # Raw material ids are rejected — the alias system is the canonical way to
    # name a material role; raw ids in map.json would let the catalog drift
    # silently. The renderer enforces the same rule.
    _validate_face_aliases(map_data, errors)

    return errors


def _validate_face_aliases(map_data: dict, errors: list[str]) -> None:
    if not MATERIAL_ALIASES:
        return  # no catalog loaded — skip rather than block all maps
    for level_idx, level in enumerate(map_data["levels"]):
        prefix = level_label(level, level_idx)
        for floor in level["floors"]:
            _check_face_aliases(floor, f"{prefix}: floor [{floor['col']}, {floor['row']}]", errors)
        for floor in level["inaccessible_floors"]:
            _check_face_aliases(floor, f"{prefix}: inaccessible_floor [{floor['col']}, {floor['row']}]", errors)
        for wall in level["walls"]:
            label = f"{prefix}: wall [{wall['c0']}, {wall['r0']}, {wall['c1']}, {wall['r1']}]"
            _check_face_aliases(wall, label, errors)
    for ramp in map_data["ramps"]:
        label = f"ramp {ramp['low']}->{ramp['high']} (level {ramp['lower_level']})"
        _check_face_aliases(ramp, label, errors)


def _check_face_aliases(seg: dict, label: str, errors: list[str]) -> None:
    for face in FACES:
        value = seg.get(face)
        if value is None or value in MATERIAL_ALIASES:
            continue
        errors.append(
            f"{label}: face {face!r} value {value!r} is not an alias; "
            f"add an alias for it in assets.json or use one of the existing aliases"
        )


def _validate_zone_rect(zone: dict, label: str, map_data: dict, errors: list[str]) -> None:
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    if not (0 <= zone["level"] < len(map_data["levels"])):
        errors.append(f"{label} has an invalid level {zone['level']}")
    c0, c1 = zone["cols"]
    r0, r1 = zone["rows"]
    if c1 <= c0 or r1 <= r0:
        errors.append(f"{label} has an empty range cols={zone['cols']} rows={zone['rows']}")
    if not (0 <= c0 and c1 <= cols and 0 <= r0 and r1 <= rows):
        errors.append(f"{label} is outside the grid: cols={zone['cols']} rows={zone['rows']}")

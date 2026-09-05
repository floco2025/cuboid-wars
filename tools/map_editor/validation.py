"""Structural validation for map data."""

from __future__ import annotations

from .constants import (
    FACES,
    ITEM_KEY_TYPE,
    ITEM_TYPES,
    LADDER_SIDES,
    LIGHT_SIDES,
    MATERIAL_ALIASES,
    MOVING_FLOOR_INSET_FRACTION,
    PLATE_TYPE_BARRIER,
    PLATE_TYPE_BRIDGE,
    PLATE_TYPES,
)
from .display import level_label
from .geometry import (
    grid_point_in_bounds,
    moving_floor_path_reaches_cell,
    moving_floor_path_reaches_edge,
    normalized_wall,
    ramp_cells,
    ramp_cells_on_level,
    ramp_error,
    wall_endpoints_for_cell_side,
)


def validate_map(map_data: dict, barrier_kinds: list[str], bridge_kinds: list[str]) -> list[str]:
    errors: list[str] = []
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    if cols <= 0 or rows <= 0:
        errors.append("grid_cols and grid_rows must be positive")
    if not map_data["levels"]:
        errors.append("at least one level is required")
    if not map_data["player_spawn_zones"]:
        errors.append("at least one player_spawn_zones entry is required by the Rust loader")
    kinds = barrier_kinds

    for idx, zone in enumerate(map_data["actor_spawn_zones"]):
        _validate_zone_rect(zone, f"actor_spawn_zones[{idx}]", map_data, errors)
        if not zone["kind"]:
            errors.append(f"actor_spawn_zones[{idx}] has empty `kind`")
        if zone["count"] < 0:
            errors.append(f"actor_spawn_zones[{idx}] has negative count")

    for idx, zone in enumerate(map_data["player_spawn_zones"]):
        _validate_zone_rect(zone, f"player_spawn_zones[{idx}]", map_data, errors)

    _validate_items(map_data, kinds, errors)
    _validate_pressure_plates(map_data, kinds, bridge_kinds, errors)

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
            if kind not in kinds:
                errors.append(f"{prefix}: barrier[{idx}] has unknown kind {kind!r}; known: [{_known(kinds)}]")
            key = tuple(normalized_wall([c0, r0, c1, r1]))
            if key in wall_endpoints_set:
                errors.append(f"{prefix}: barrier[{idx}] {list(key)} overlaps a wall")
            if key in barrier_seen:
                errors.append(f"{prefix}: barrier[{idx}] {list(key)} duplicates another barrier")
            barrier_seen.add(key)

        slab_set = floor_set | {(f["col"], f["row"]) for f in level["inaccessible_floors"]}
        ramp_set = ramp_cells_on_level(map_data["ramps"], level_idx)
        bridge_seen: set[tuple[int, int]] = set()
        for idx, bridge in enumerate(level.get("light_bridges", [])):
            c, r, kind = bridge["col"], bridge["row"], bridge.get("kind")
            label = f"{prefix}: light_bridge[{idx}]"
            if not (0 <= c < cols and 0 <= r < rows):
                errors.append(f"{label} [{c}, {r}] is outside the grid")
            if kind not in bridge_kinds:
                errors.append(f"{label} has unknown kind {kind!r}; known: [{_known(bridge_kinds)}]")
            if (c, r) in slab_set:
                errors.append(f"{label} [{c}, {r}] sits on a floor")
            if (c, r) in ramp_set:
                errors.append(f"{label} [{c}, {r}] sits on a ramp")
            if (c, r) in bridge_seen:
                errors.append(f"{label} [{c}, {r}] duplicates another light bridge")
            bridge_seen.add((c, r))

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

    _validate_ladders(map_data, errors)
    _validate_moving_floors(map_data, errors)

    # Face values on walls, floors, ramps must be aliases (assets.json::aliases).
    # Raw material ids are rejected — the alias system is the canonical way to
    # name a material role; raw ids in map.json would let the catalog drift
    # silently. The renderer enforces the same rule.
    _validate_face_aliases(map_data, errors)

    return errors


def _known(kinds: list[str]) -> str:
    return ", ".join(kinds) or "(none listed)"


def _validate_ladders(map_data: dict, errors: list[str]) -> None:
    # Deliberately permissive (no wall/floor/clear-edge requirements — the
    # climb mechanic handles every surrounding); only structural integrity
    # is checked, mirroring the Rust loader.
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    level_count = len(map_data["levels"])
    for idx, ladder in enumerate(map_data.get("ladders", [])):
        label = f"ladders[{idx}]"
        col, row, side = ladder["col"], ladder["row"], ladder["side"]
        lower, levels = ladder["lower_level"], ladder["levels"]
        if not (0 <= col < cols and 0 <= row < rows):
            errors.append(f"{label} [{col}, {row}] is outside the grid")
        if side not in LADDER_SIDES:
            errors.append(f"{label} has invalid side {side!r}")
        if levels < 1:
            errors.append(f"{label} must span at least 1 storey")
        if not (0 <= lower and lower + levels < level_count):
            errors.append(
                f"{label} spans levels {lower}..{lower + levels} but the map has {level_count} level(s)"
            )
        # Undirected edge: an edge holds at most one ladder — a mirrored
        # pair would put two ladders' geometry on the same edge, so it
        # counts as the same ladder twice even though only the front climbs.
        edge = wall_endpoints_for_cell_side(col, row, side) if side in LADDER_SIDES else None
        for other_idx, other in enumerate(map_data["ladders"][:idx]):
            if (
                edge is not None
                and wall_endpoints_for_cell_side(other["col"], other["row"], other["side"]) == edge
                and other["lower_level"] < lower + levels
                and lower < other["lower_level"] + other["levels"]
            ):
                errors.append(f"{label} overlaps ladders[{other_idx}] on the same edge")


def _validate_moving_floors(map_data: dict, errors: list[str]) -> None:
    # Mirrors the Rust loader: the whole path must be clear — no slab, ramp,
    # or light bridge on any cell the tile's body sweeps over, on any storey
    # it passes through, and no wall or barrier edge its body sweeps through
    # (an edge along the side of its path is fine).
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    level_count = len(map_data["levels"])
    seen: set[tuple] = set()
    for idx, floor in enumerate(map_data.get("moving_floors", [])):
        label = f"moving_floors[{idx}]"
        level, to_level = floor["level"], floor["to_level"]
        start, end = floor["from"], floor["to"]
        if not (0 <= level < level_count and 0 <= to_level < level_count):
            errors.append(f"{label} spans levels {level}..{to_level} but the map has {level_count} level(s)")
            continue
        if not all(0 <= c < cols and 0 <= r < rows for c, r in (start, end)):
            errors.append(f"{label} {start}->{end} is outside the grid")
            continue
        if start == end and level == to_level:
            errors.append(f"{label} at level {level} {start} must travel")
        if not floor["speed"] > 0:
            errors.append(f"{label} needs a positive speed")
        if floor["pause_secs"] < 0 or floor["phase_secs"] < 0:
            errors.append(f"{label} has a negative pause or phase")
        key = (level, tuple(start))
        if key in seen:
            errors.append(f"{label} duplicates a moving floor starting at level {level} {start}")
        seen.add(key)
        half = 0.5 - MOVING_FLOOR_INSET_FRACTION

        def relation(cell: tuple[int, int], level_idx: int) -> str:
            if list(cell) == start and level_idx == level:
                return "starts on"
            if list(cell) == end and level_idx == to_level:
                return "ends on"
            return "passes over"

        for level_idx in range(min(level, to_level), max(level, to_level) + 1):
            level_data = map_data["levels"][level_idx]
            prefix = level_label(level_data, level_idx)
            slabs = level_data["floors"] + level_data["inaccessible_floors"]
            for slab in slabs:
                cell = (slab["col"], slab["row"])
                if moving_floor_path_reaches_cell(floor, cell, half):
                    errors.append(
                        f"{label} {relation(cell, level_idx)} a floor at {prefix} [{cell[0]}, {cell[1]}]; "
                        "a moving floor needs floorless cells wherever it goes"
                    )
                    break
            for bridge in level_data.get("light_bridges", []):
                cell = (bridge["col"], bridge["row"])
                if moving_floor_path_reaches_cell(floor, cell, half):
                    errors.append(
                        f"{label} {relation(cell, level_idx)} a light bridge at {prefix} [{cell[0]}, {cell[1]}]"
                    )
                    break
            for cell in sorted(ramp_cells_on_level(map_data["ramps"], level_idx)):
                if moving_floor_path_reaches_cell(floor, cell, half):
                    errors.append(f"{label} {relation(cell, level_idx)} a ramp at {prefix} [{cell[0]}, {cell[1]}]")
                    break
            for list_name, noun in (("walls", "wall"), ("barriers", "barrier")):
                for edge in level_data.get(list_name, []):
                    if moving_floor_path_reaches_edge(floor, [edge["c0"], edge["r0"], edge["c1"], edge["r1"]], half):
                        errors.append(
                            f"{label} crosses a {noun} at {prefix} "
                            f"[{edge['c0']}, {edge['r0']}, {edge['c1']}, {edge['r1']}]"
                        )
                        break
        _check_face_aliases(floor, f"{label} {start}->{end}", errors)


def _validate_pressure_plates(map_data: dict, kinds: list[str], bridge_kinds: list[str], errors: list[str]) -> None:
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    seen: set[tuple] = set()
    for idx, plate in enumerate(map_data.get("pressure_plates", [])):
        label = f"pressure_plates[{idx}]"
        level_idx, col, row = plate["level"], plate["col"], plate["row"]
        if not (0 <= level_idx < len(map_data["levels"])):
            errors.append(f"{label} has an invalid level {level_idx}")
            continue
        if not (0 <= col < cols and 0 <= row < rows):
            errors.append(f"{label} [{col}, {row}] is outside the grid")
            continue
        plate_type = plate.get("type")
        if plate_type == PLATE_TYPE_BARRIER:
            kind = plate.get("kind")
            if kind not in kinds:
                errors.append(f"{label} has unknown barrier kind {kind!r}; known: [{_known(kinds)}]")
        elif plate_type == PLATE_TYPE_BRIDGE:
            kind = plate.get("kind")
            if kind not in bridge_kinds:
                errors.append(f"{label} has unknown bridge kind {kind!r}; known: [{_known(bridge_kinds)}]")
        elif plate_type not in PLATE_TYPES:
            known = ", ".join(PLATE_TYPES)
            errors.append(f"{label} has unknown type {plate_type!r}; known: [{known}]")
        elif "kind" in plate:
            errors.append(f"{label} ({plate_type}) must not have `kind` — only barrier and bridge plates take one")
        level = map_data["levels"][level_idx]
        if any(b["col"] == col and b["row"] == row for b in level.get("light_bridges", [])):
            errors.append(f"{label} [{col}, {row}] sits on a light bridge")
        # The Rust loader dedupes per purpose: plates of different purposes
        # may share a cell, two identical plates may not.
        key = (level_idx, col, row, plate_type, plate.get("kind"))
        if key in seen:
            errors.append(f"{label} duplicates a plate at level {level_idx} [{col}, {row}]")
        seen.add(key)


def _validate_items(map_data: dict, kinds: list[str], errors: list[str]) -> None:
    cols = map_data["grid_cols"]
    rows = map_data["grid_rows"]
    seen_cells: set[tuple[int, int, int]] = set()
    for idx, item in enumerate(map_data.get("items", [])):
        label = f"items[{idx}]"
        level_idx, col, row = item["level"], item["col"], item["row"]
        if not (0 <= level_idx < len(map_data["levels"])):
            errors.append(f"{label} has an invalid level {level_idx}")
            continue
        if not (0 <= col < cols and 0 <= row < rows):
            errors.append(f"{label} [{col}, {row}] is outside the grid")
            continue
        item_type = item.get("type")
        if item_type == ITEM_KEY_TYPE:
            kind = item.get("kind")
            if kind not in kinds:
                errors.append(f"{label} has unknown key kind {kind!r}; known: [{_known(kinds)}]")
        elif item_type not in ITEM_TYPES:
            known = ", ".join(ITEM_TYPES)
            errors.append(f"{label} has unknown type {item_type!r}; known: [{known}]")
        elif "kind" in item:
            errors.append(f"{label} ({item_type}) must not have `kind` — only key items take one")
        level = map_data["levels"][level_idx]
        if (col, row) not in {(f["col"], f["row"]) for f in level["floors"]}:
            errors.append(f"{label} [{col}, {row}] has no regular floor")
        # The Rust loader rejects items on ramp cells; `has_ramp` marks the
        # lower level's footprint cells only.
        for ramp in map_data["ramps"]:
            if ramp["lower_level"] == level_idx and (col, row) in ramp_cells(ramp):
                errors.append(f"{label} [{col}, {row}] is inside a ramp footprint")
                break
        if (level_idx, col, row) in seen_cells:
            errors.append(f"{label} duplicates an item at level {level_idx} [{col}, {row}]")
        seen_cells.add((level_idx, col, row))


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

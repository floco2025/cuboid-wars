"""Deterministic JSON formatting for map files."""

from __future__ import annotations

import json

from .normalization import compact_face_materials


def json_scalar(value) -> str:
    return json.dumps(value, separators=(",", ": "))


def with_trailing_comma(lines: list[str]) -> list[str]:
    # Pure: return a new list with a trailing comma on the final entry.
    # The caller almost always splat-spreads the result, so the extra list
    # is short-lived.
    if not lines:
        return []
    return [*lines[:-1], lines[-1] + ","]


def _ramp_body(ramp: dict) -> str:
    body = {"lower_level": ramp["lower_level"], "low": ramp["low"], "high": ramp["high"], **compact_face_materials(ramp)}
    return _inline_object_body(body)


def _actor_spawn_zone_body(zone: dict) -> str:
    body = {"level": zone["level"], "cols": zone["cols"], "rows": zone["rows"], "kind": zone["kind"], "count": zone["count"]}
    return _inline_object_body(body)


def _player_spawn_zone_body(zone: dict) -> str:
    return _inline_object_body({"level": zone["level"], "cols": zone["cols"], "rows": zone["rows"]})


def _checkpoint_body(zone: dict) -> str:
    return _inline_object_body({"level": zone["level"], "cols": zone["cols"], "rows": zone["rows"], "type": zone["type"]})


def _pressure_plate_body(plate: dict) -> str:
    body = {"level": plate["level"], "col": plate["col"], "row": plate["row"], "type": plate["type"]}
    if "kind" in plate:
        body["kind"] = plate["kind"]
    return _inline_object_body(body)


def format_map_file(wrapper: dict) -> str:
    map_data = wrapper["map"]
    lines = [
        "{",
        '  "map": {',
        f'    "grid_cols": {map_data["grid_cols"]},',
        f'    "grid_rows": {map_data["grid_rows"]},',
        *with_trailing_comma(format_object_array("actor_spawn_zones", map_data["actor_spawn_zones"], _actor_spawn_zone_body, 4)),
        *with_trailing_comma(format_object_array("player_spawn_zones", map_data["player_spawn_zones"], _player_spawn_zone_body, 4)),
        *with_trailing_comma(format_object_array("checkpoints", map_data.get("checkpoints", []), _checkpoint_body, 4)),
        *with_trailing_comma(format_object_array("pressure_plates", map_data.get("pressure_plates", []), _pressure_plate_body, 4)),
        *with_trailing_comma(format_object_array("items", map_data.get("items", []), _item_body, 4)),
        '    "levels": [',
    ]

    for level_idx, level in enumerate(map_data["levels"]):
        lines.extend(
            [
                "      {",
                f'        "name": {json_scalar(level["name"])},',
                *with_trailing_comma(format_object_array("floors", level["floors"], _floor_body, 8)),
                *with_trailing_comma(
                    format_object_array("inaccessible_floors", level["inaccessible_floors"], _floor_body, 8)
                ),
                *with_trailing_comma(format_object_array("grass", level.get("grass", []), _grass_body, 8)),
                *with_trailing_comma(format_object_array("walls", level["walls"], _wall_body, 8)),
                *with_trailing_comma(format_object_array("barriers", level.get("barriers", []), _barrier_body, 8)),
                *with_trailing_comma(format_object_array("erasers", level.get("erasers", []), _eraser_body, 8)),
                *with_trailing_comma(
                    format_object_array("light_bridges", level.get("light_bridges", []), _light_bridge_body, 8)
                ),
                *format_object_array("lights", level.get("lights", []), _light_body, 8),
                "      }" + ("," if level_idx + 1 < len(map_data["levels"]) else ""),
            ]
        )

    lines.append("    ],")
    lines.extend(with_trailing_comma(format_object_array("ladders", map_data.get("ladders", []), _ladder_body, 4)))
    lines.extend(with_trailing_comma(format_object_array("ramps", map_data["ramps"], _ramp_body, 4)))
    lines.extend(format_object_array("nested_maps", map_data.get("nested_maps", []), _nested_map_body, 4))
    if "nested_geometry" in map_data:
        lines[-1] += ","
        lines.append('    "nested_geometry": {')
        definitions = list(map_data["nested_geometry"].items())
        for index, (name, geometry) in enumerate(definitions):
            body = format_map_file({"map": geometry}).splitlines()[2:-2]
            lines.append(f"      {json_scalar(name)}: {{")
            lines.extend("    " + line for line in body)
            lines.append("      }" + ("," if index + 1 < len(definitions) else ""))
        lines.append("    }")
    lines.append("  }")
    lines.append("}")
    return "\n".join(lines)


def format_object_array(name: str, items: list, render_body, indent: int) -> list[str]:
    """Render a JSON array of one-line objects under a given key.

    `render_body(item)` returns the inline content between the braces — e.g.
    `'"col": 5, "row": 3'`. Empty arrays render as `"name": []` on one line.
    """
    pad = " " * indent
    inner = " " * (indent + 2)
    if not items:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    last = len(items) - 1
    for idx, item in enumerate(items):
        comma = "," if idx < last else ""
        lines.append(f"{inner}{{{render_body(item)}}}{comma}")
    lines.append(f"{pad}]")
    return lines


def _floor_body(floor: dict) -> str:
    body = {"col": floor["col"], "row": floor["row"], **compact_face_materials(floor)}
    return _inline_object_body(body)


def _grass_body(grass: dict) -> str:
    return _inline_object_body({"col": grass["col"], "row": grass["row"]})


def _wall_body(wall: dict) -> str:
    body = {
        "c0": wall["c0"], "r0": wall["r0"], "c1": wall["c1"], "r1": wall["r1"],
        **compact_face_materials(wall),
    }
    return _inline_object_body(body)


def _light_body(light: dict) -> str:
    body = {"col": light["col"], "row": light["row"], "side": light["side"], "kind": light.get("kind", "")}
    return _inline_object_body(body)


def _item_body(item: dict) -> str:
    body = {"level": item["level"], "col": item["col"], "row": item["row"], "type": item["type"]}
    if "kind" in item:
        body["kind"] = item["kind"]
    return _inline_object_body(body)


def _ladder_body(ladder: dict) -> str:
    body = {
        "lower_level": ladder["lower_level"],
        "col": ladder["col"], "row": ladder["row"], "side": ladder["side"],
        "levels": ladder["levels"],
    }
    return _inline_object_body(body)


def _nested_map_body(entry: dict) -> str:
    body = {
        "map": entry["map"],
        "level": entry["level"],
        "from": entry["from"],
        "to": entry["to"],
        "to_level": entry["to_level"],
        "travel_secs": entry["travel_secs"],
        "pause_secs": entry["pause_secs"],
        "phase_secs": entry["phase_secs"],
        "from_nudge": entry["from_nudge"],
        "to_nudge": entry["to_nudge"],
    }
    return _inline_object_body(body)


def _barrier_body(barrier: dict) -> str:
    body = {
        "c0": barrier["c0"], "r0": barrier["r0"], "c1": barrier["c1"], "r1": barrier["r1"],
        "kind": barrier["kind"],
    }
    return _inline_object_body(body)


def _light_bridge_body(bridge: dict) -> str:
    return _inline_object_body({"col": bridge["col"], "row": bridge["row"], "kind": bridge["kind"]})


def _inline_object_body(body: dict) -> str:
    # `json.dumps` brackets the whole object; strip the outer braces so the
    # caller can wrap with its own punctuation/comma.
    return json.dumps(body, separators=(", ", ": "))[1:-1]


def _eraser_body(eraser: dict) -> str:
    return _inline_object_body({key: eraser[key] for key in ("c0", "r0", "c1", "r1")})

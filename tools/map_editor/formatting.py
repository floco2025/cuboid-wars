"""Deterministic JSON formatting for map files."""

from __future__ import annotations

import json

from .geometry import compact_face_materials


def json_scalar(value) -> str:
    return json.dumps(value, separators=(",", ": "))


def format_point(point: list[int]) -> str:
    return "[" + ", ".join(str(v) for v in point) + "]"


def with_trailing_comma(lines: list[str]) -> list[str]:
    # Pure: return a new list with a trailing comma on the final entry.
    # The caller almost always splat-spreads the result, so the extra list
    # is short-lived.
    if not lines:
        return []
    return [*lines[:-1], lines[-1] + ","]


def _ramp_body(ramp: dict) -> str:
    materials = compact_face_materials(ramp)
    materials_part = ""
    if materials:
        materials_part = ", " + ", ".join(
            f'"{key}": {json.dumps(value)}' for key, value in materials.items()
        )
    return (
        f'"lower_level": {ramp["lower_level"]}, '
        f'"low": {format_point(ramp["low"])}, '
        f'"high": {format_point(ramp["high"])}{materials_part}'
    )


def _zone_rect_fragment(zone: dict) -> str:
    return (
        f'"level": {zone["level"]}, '
        f'"cols": [{zone["cols"][0]}, {zone["cols"][1]}], '
        f'"rows": [{zone["rows"][0]}, {zone["rows"][1]}]'
    )


def format_actor_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    def render(zone: dict) -> str:
        return (
            f"{{{_zone_rect_fragment(zone)}, "
            f'"kind": {json.dumps(zone["kind"])}, '
            f'"count": {zone["count"]}}}'
        )

    return _format_zone_list("actor_spawn_zones", zones, indent, render)


def format_cookie_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    return _format_zone_list(
        "cookie_spawn_zones",
        zones,
        indent,
        lambda zone: f"{{{_zone_rect_fragment(zone)}}}",
    )


def format_key_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    return _format_zone_list(
        "key_spawn_zones",
        zones,
        indent,
        lambda zone: f'{{{_zone_rect_fragment(zone)}, "kind": {json.dumps(zone["kind"])}}}',
    )


def format_pressure_plates(plates: list[dict], indent: int) -> list[str]:
    return _format_zone_list(
        "pressure_plates",
        plates,
        indent,
        lambda plate: (
            f'{{"level": {plate["level"]}, '
            f'"col": {plate["col"]}, '
            f'"row": {plate["row"]}, '
            f'"kind": {json.dumps(plate["kind"])}}}'
        ),
    )


def format_player_spawn_zones(zones: list[dict], indent: int) -> list[str]:
    return _format_zone_list(
        "player_spawn_zones",
        zones,
        indent,
        lambda zone: f"{{{_zone_rect_fragment(zone)}}}",
    )


def _format_zone_list(name: str, zones: list[dict], indent: int, render_zone) -> list[str]:
    pad = " " * indent
    inner = " " * (indent + 2)
    if not zones:
        return [f'{pad}"{name}": []']
    lines = [f'{pad}"{name}": [']
    for idx, zone in enumerate(zones):
        comma = "," if idx + 1 < len(zones) else ""
        lines.append(f"{inner}{render_zone(zone)}{comma}")
    lines.append(f"{pad}]")
    return lines


def format_map_file(wrapper: dict) -> str:
    map_data = wrapper["map"]
    lines = [
        "{",
        f'  "version": {wrapper["version"]},',
        '  "map": {',
        f'    "grid_cols": {map_data["grid_cols"]},',
        f'    "grid_rows": {map_data["grid_rows"]},',
        *with_trailing_comma(format_actor_spawn_zones(map_data["actor_spawn_zones"], 4)),
        *with_trailing_comma(format_player_spawn_zones(map_data["player_spawn_zones"], 4)),
        *with_trailing_comma(format_cookie_spawn_zones(map_data["cookie_spawn_zones"], 4)),
        *with_trailing_comma(format_key_spawn_zones(map_data["key_spawn_zones"], 4)),
        *with_trailing_comma(format_pressure_plates(map_data.get("pressure_plates", []), 4)),
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
                *with_trailing_comma(format_object_array("walls", level["walls"], _wall_body, 8)),
                *with_trailing_comma(format_object_array("barriers", level.get("barriers", []), _barrier_body, 8)),
                *format_object_array("lights", level.get("lights", []), _light_body, 8),
                "      }" + ("," if level_idx + 1 < len(map_data["levels"]) else ""),
            ]
        )

    lines.append("    ],")
    lines.extend(format_object_array("ramps", map_data["ramps"], _ramp_body, 4))
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


def _wall_body(wall: dict) -> str:
    body = {
        "c0": wall["c0"], "r0": wall["r0"], "c1": wall["c1"], "r1": wall["r1"],
        **compact_face_materials(wall),
    }
    return _inline_object_body(body)


def _light_body(light: dict) -> str:
    body = {"col": light["col"], "row": light["row"], "side": light["side"]}
    return _inline_object_body(body)


def _barrier_body(barrier: dict) -> str:
    body = {
        "c0": barrier["c0"], "r0": barrier["r0"], "c1": barrier["c1"], "r1": barrier["r1"],
        "kind": barrier["kind"],
    }
    return _inline_object_body(body)


def _inline_object_body(body: dict) -> str:
    # `json.dumps` brackets the whole object; strip the outer braces so the
    # caller can wrap with its own punctuation/comma.
    return json.dumps(body, separators=(", ", ": "))[1:-1]

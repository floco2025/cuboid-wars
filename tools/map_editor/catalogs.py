"""The catalogs the editor reads from the gameplay and asset configuration:
the map registry, each map's settings, and the global actor and wall-light
kinds. Catalog entries are metadata; no texture image is ever loaded."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path

from .constants import ASSETS_PATH, GAMEPLAY_PATH, MAP_NAME_RE, MAPS_DIR


def load_wall_light_kinds() -> list[str]:
    with ASSETS_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["wall_lights"])


def load_actor_kinds() -> list[str]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["actors"]["kinds"])


def load_immovable_actor_kinds() -> set[str]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        return {name for name, kind in json.load(handle)["actors"]["kinds"].items() if kind["immovable"]}


HEX_COLOR = re.compile(r"#[0-9a-fA-F]{6}")


# A map's kind catalog from its gameplay settings, in catalog order: id → "#rrggbb".
def load_map_kinds(map_name: str, key: str) -> dict[str, str]:
    map_settings = load_map_settings(map_name)
    source = map_settings_path(map_name)
    if key not in map_settings:
        raise ValueError(f"{source}: {key} is required; use [] when the map has none")
    value = map_settings[key]
    if not isinstance(value, list):
        raise ValueError(f"{source}: {key} must be an array of {{id, color}} objects")
    kinds: dict[str, str] = {}
    for idx, entry in enumerate(value):
        path = f"{source}: {key}[{idx}]"
        if not isinstance(entry, dict) or not isinstance(entry.get("id"), str) or not isinstance(entry.get("color"), str):
            raise ValueError(f"{path} must be an object with string `id` and `color`")
        kind, color = entry["id"], entry["color"]
        if not kind:
            raise ValueError(f"{path}.id is empty")
        if kind in kinds:
            raise ValueError(f"{path}.id duplicates {kind!r}")
        if not HEX_COLOR.fullmatch(color):
            raise ValueError(f"{path}.color must look like #rrggbb, got {color!r}")
        kinds[kind] = color
    return kinds


def load_map_barrier_kinds(map_name: str) -> dict[str, str]:
    return load_map_kinds(map_name, "barrier_kinds")


SWITCH_ACTIVATIONS = ("momentary", "toggle", "auto")
SWITCH_RESETS = ("never", "solo", "any", "all")
SWITCH_HOLDS = ("any", "everyone")


# A map's switch catalog from its gameplay settings, in catalog order: the
# ids a plate, a kind, an actor zone, or a nested map may name.
def load_map_switches(map_name: str) -> list[str]:
    map_settings = load_map_settings(map_name)
    source = map_settings_path(map_name)
    if "switches" not in map_settings:
        raise ValueError(f"{source}: switches is required; use [] when the map has none")
    value = map_settings["switches"]
    if not isinstance(value, list):
        raise ValueError(f"{source}: switches must be an array of {{id, activation, reset_on_player_death}} objects")
    switches: list[str] = []
    for idx, entry in enumerate(value):
        path = f"{source}: switches[{idx}]"
        if not isinstance(entry, dict) or not isinstance(entry.get("id"), str):
            raise ValueError(f"{path} must be an object with a string `id`")
        switch = entry["id"]
        if not switch:
            raise ValueError(f"{path}.id is empty")
        if switch in switches:
            raise ValueError(f"{path}.id duplicates {switch!r}")
        for key, allowed in (("activation", SWITCH_ACTIVATIONS), ("reset_on_player_death", SWITCH_RESETS)):
            if entry.get(key) not in allowed:
                raise ValueError(f"{path}.{key} must be one of {', '.join(allowed)}, got {entry.get(key)!r}")
        if entry.get("held", "any") not in SWITCH_HOLDS:
            raise ValueError(f"{path}.held must be one of {', '.join(SWITCH_HOLDS)}, got {entry.get('held')!r}")
        color = entry.get("plate_color")
        if color is not None and (not isinstance(color, str) or not HEX_COLOR.fullmatch(color)):
            raise ValueError(f"{path}.plate_color must look like #rrggbb, got {color!r}")
        switches.append(switch)
    return switches


def load_map_plate_colors(map_name: str) -> dict[str, str]:
    settings = load_map_settings(map_name)
    with ASSETS_PATH.open(encoding="utf-8") as handle:
        default_color = json.load(handle)["pressure_plate"]["default_color"]
    kinds = [*settings["barrier_kinds"], *settings["bridge_kinds"]]
    return {
        switch["id"]: switch.get("plate_color") or next(
            (kind["color"] for kind in kinds if kind.get("switch") == switch["id"]),
            default_color,
        )
        for switch in settings["switches"]
    }


def load_map_bridge_kinds(map_name: str) -> dict[str, str]:
    return load_map_kinds(map_name, "bridge_kinds")


def load_map_wall_width_cells(map_name: str) -> float:
    """One wall width in cells, the unit a nested map's nudge is drawn in."""
    map_settings = load_map_settings(map_name)
    geometry = map_settings["geometry"]
    return float(geometry["wall_thickness"]) / float(geometry["grid_cell_size"])


def load_texture_catalog(host: str) -> dict[str, bool]:
    settings = load_map_settings(host)
    path = f"{map_settings_path(host)}: textures"
    value = settings.get("textures")
    if not isinstance(value, dict):
        raise ValueError(f"{path} must be an object of aliases with a portalable boolean")
    result = {}
    for alias, entry in value.items():
        if not alias.strip() or not isinstance(entry, dict) or type(entry.get("portalable")) is not bool:
            raise ValueError(f"{path}.{alias}: expected a nonempty alias and an explicit portalable boolean")
        result[alias] = entry["portalable"]
    return dict(sorted(result.items()))


def read_settings_json(path: Path) -> dict:
    try:
        with path.open(encoding="utf-8") as handle:
            value = json.load(handle)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def list_map_names() -> list[str]:
    gameplay = read_settings_json(GAMEPLAY_PATH)
    names = gameplay.get("maps")
    if not isinstance(names, list) or not names:
        raise ValueError(f"{GAMEPLAY_PATH}: maps must be a nonempty array of map names")
    seen = set()
    for name in names:
        if not isinstance(name, str) or not MAP_NAME_RE.fullmatch(name):
            raise ValueError(f"{GAMEPLAY_PATH}: invalid map name {name!r}")
        if name in seen:
            raise ValueError(f"{GAMEPLAY_PATH}: duplicate map name {name!r}")
        seen.add(name)
    default_map = gameplay.get("default_map")
    if not isinstance(default_map, str) or default_map not in seen:
        raise ValueError(f"{GAMEPLAY_PATH}: default_map must name a registered map")
    return sorted(names)


def map_settings_path(name: str) -> Path:
    if not MAP_NAME_RE.fullmatch(name):
        raise ValueError(f"Invalid map name {name!r}")
    return MAPS_DIR / name / "settings.json"


def map_layout_path(name: str) -> Path:
    return map_settings_path(name).with_name("layout.json")


def map_name_from_path(path: Path) -> str:
    name = path.parent.name
    if path.name != "layout.json" or not MAP_NAME_RE.fullmatch(name):
        raise ValueError(f"Map layout path must end in <map name>/layout.json: {path}")
    return name


def load_map_settings(name: str) -> dict:
    if name not in list_map_names():
        raise ValueError(f"Map {name!r} is not registered in {GAMEPLAY_PATH}.")
    return read_settings_json(map_settings_path(name))


def require_map_settings(name: str) -> None:
    load_map_settings(name)


@dataclass(frozen=True)
class MapCatalogs:
    """Everything one map's `settings.json` gives the editor: kind ids in
    catalog order with their colours, the wall width nudges are drawn in,
    the texture aliases with their portal permission, and the switch ids."""

    barrier_kind_colors: dict[str, str]
    bridge_kind_colors: dict[str, str]
    wall_width_cells: float
    texture_catalog: dict[str, bool]
    # The switch ids in catalog order.
    switches: list[str] = field(default_factory=list)
    plate_colors: dict[str, str] = field(default_factory=dict)

    @classmethod
    def load(cls, map_name: str) -> "MapCatalogs":
        return cls(
            load_map_barrier_kinds(map_name),
            load_map_bridge_kinds(map_name),
            load_map_wall_width_cells(map_name),
            load_texture_catalog(map_name),
            load_map_switches(map_name),
            load_map_plate_colors(map_name),
        )

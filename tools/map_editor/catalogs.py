"""The catalogs the editor reads: the map registry, each map's layout kinds
and settings, and the global actor and wall-light kinds. Catalog entries are
metadata; no texture image is ever loaded."""

from __future__ import annotations

import json
import re
from math import isfinite
from dataclasses import dataclass, field, replace
from pathlib import Path

from .constants import ASSETS_PATH, GAMEPLAY_PATH, ITEM_TYPES, MAP_NAME_RE, MAPS_DIR, POWER_UP_TYPES
from .core import call


def pickup_types(settings: dict, source: str) -> tuple[str, ...]:
    rules = settings.get("power_ups")
    if not isinstance(rules, dict) or rules.keys() != set(POWER_UP_TYPES):
        raise ValueError(f"{source}: power_ups must define {', '.join(POWER_UP_TYPES)}")
    always = set()
    for kind, rule in rules.items():
        path = f"{source}: power_ups.{kind}"
        if not isinstance(rule, dict):
            raise ValueError(f"{path} must be an object")
        if rule.get("mode") == "always" and rule.keys() == {"mode"}:
            always.add(kind)
        elif rule.get("mode") == "pickup" and rule.keys() == {"mode", "duration_secs"}:
            duration = rule["duration_secs"]
            if duration is not None and (type(duration) not in (int, float) or not isfinite(duration) or duration <= 0):
                raise ValueError(f"{path}.duration_secs must be positive seconds or null")
        else:
            raise ValueError(f"{path} needs mode always, or mode pickup with duration_secs")
    random_items = settings.get("random_items")
    if isinstance(random_items, dict):
        for kind in random_items.get("weights", {}):
            if kind in always:
                raise ValueError(f"{source}: random_items.weights.{kind} is always active and cannot be a pickup")
    return tuple(kind for kind in ITEM_TYPES if kind not in always)


def load_wall_light_kinds() -> list[str]:
    with ASSETS_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["wall_lights"])


def load_actor_kinds() -> list[str]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["actors"])


HEX_COLOR = re.compile(r"#[0-9a-fA-F]{6}")


# A layout's kind catalog in catalog order: id → "#rrggbb". Malformed
# entries are validation's business and are left out here.
def kind_colors(root: dict, key: str) -> dict[str, str]:
    entries = root.get(key, [])
    if not isinstance(entries, list):
        return {}
    return {
        entry["id"]: entry["color"]
        for entry in entries
        if isinstance(entry, dict) and isinstance(entry.get("id"), str) and isinstance(entry.get("color"), str)
    }


SWITCH_ACTIVATIONS = ("momentary", "toggle", "auto")
SWITCH_RESETS = ("never", "solo", "any", "all")
SWITCH_HOLDS = ("any", "everyone")


def switch_entries(root: dict) -> list[dict]:
    entries = root.get("switches", [])
    return (
        [entry for entry in entries if isinstance(entry, dict) and isinstance(entry.get("id"), str)]
        if isinstance(entries, list)
        else []
    )


def switch_colors(root: dict, barriers: dict[str, str], bridges: dict[str, str]) -> dict[str, str]:
    from .validation import placed_definitions

    with ASSETS_PATH.open(encoding="utf-8") as handle:
        default_color = json.load(handle)["pressure_plate"]["default_color"]
    geometries = [root, *placed_definitions(root, root.get("nested_geometry", {})).values()]
    targets = [
        (entry.get("switch"), colors.get(entry.get("kind")))
        for name, colors in (("barriers", barriers), ("light_bridges", bridges))
        for kind in colors
        for geometry in geometries
        for level in geometry.get("levels", [])
        for entry in level.get(name, [])
        if entry.get("kind") == kind
    ]

    def override(entry):
        color = entry.get("color")
        return color if isinstance(color, str) and HEX_COLOR.fullmatch(color) else None

    return {
        entry["id"]: override(entry)
        or next(
            (color for switch, color in targets if switch == entry["id"] and color),
            default_color,
        )
        for entry in switch_entries(root)
    }


def load_map_geometry(map_name: str) -> tuple[float, float, float, float]:
    settings = load_map_settings(map_name)
    source = str(map_settings_path(map_name))
    cell, level_height, wall, floor = (
        setting_number(settings, source, f"geometry.{key}")
        for key in ("grid_cell_size", "level_height", "wall_thickness", "floor_thickness")
    )
    return cell, level_height, wall / cell, floor


def load_texture_catalog(host: str) -> dict[str, bool]:
    settings = load_map_settings(host)
    path = f"{map_settings_path(host)}: textures"
    value = settings.get("textures")
    if not isinstance(value, dict):
        raise ValueError(f"{path} must be an object of aliases with a material and a portalable boolean")
    result = {}
    for alias, entry in value.items():
        if (
            not alias.strip()
            or not isinstance(entry, dict)
            or not isinstance(entry.get("material"), str)
            or not entry["material"].strip()
            or type(entry.get("portalable")) is not bool
        ):
            raise ValueError(
                f"{path}.{alias}: expected a nonempty alias, a material, and an explicit portalable boolean"
            )
        result[alias] = entry["portalable"]
    return dict(sorted(result.items()))


# A finite number at a dotted path, positive unless `allow_zero`; errors name the source file and path.
def setting_number(settings: dict, source: str, path: str, *, allow_zero: bool = False) -> float:
    value = settings
    for key in path.split("."):
        value = value.get(key) if isinstance(value, dict) else None
    if type(value) not in (int, float) or not isfinite(value) or (value < 0 if allow_zero else value <= 0):
        requirement = "nonnegative" if allow_zero else "positive"
        raise ValueError(f"{source}: {path} must be a finite {requirement} number")
    return float(value)


def read_settings_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def parse_settings_json(text: str, path: Path) -> dict:
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def read_settings_json(path: Path) -> dict:
    return parse_settings_json(read_settings_text(path), path)


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


# The map's effective settings: the gameplay.json defaults with the map
# file's overrides applied by the shared map core rule.
def load_map_settings(name: str) -> dict:
    if name not in list_map_names():
        raise ValueError(f"Map {name!r} is not registered in {GAMEPLAY_PATH}.")
    path = map_settings_path(name)
    settings = read_settings_json(path)
    for key in ("grounds", "random_items", "placed_items"):
        if key not in settings or (settings[key] is not None and not isinstance(settings[key], dict)):
            raise ValueError(f"{path}: {key} requires an object or explicit null")
    defaults = {
        key: value for key, value in read_settings_json(GAMEPLAY_PATH).items() if key not in ("default_map", "maps")
    }
    try:
        return call("merge_map_settings", defaults, settings)
    except ValueError as error:
        raise ValueError(f"{path}: {error}") from None


def require_map_settings(name: str) -> None:
    load_map_settings(name)


@dataclass(frozen=True)
class MapCatalogs:
    """The host map's appearance, controls, materials, and drawing settings."""

    barrier_kind_colors: dict[str, str]
    bridge_kind_colors: dict[str, str]
    wall_width_cells: float
    texture_catalog: dict[str, bool]
    # The switch ids in catalog order.
    switches: list[str] = field(default_factory=list)
    switch_colors: dict[str, str] = field(default_factory=dict)
    grid_cell_size: float = 0.0
    level_height: float = 0.0
    floor_thickness: float = 0.0
    pickup_types: tuple[str, ...] = ITEM_TYPES

    # The layout owns the kinds and switches; the rest stays as loaded.
    def for_layout(self, root: dict) -> "MapCatalogs":
        barriers = kind_colors(root, "barrier_kinds")
        bridges = kind_colors(root, "bridge_kinds")
        return replace(
            self,
            barrier_kind_colors=barriers,
            bridge_kind_colors=bridges,
            switches=[entry["id"] for entry in switch_entries(root)],
            switch_colors=switch_colors(root, barriers, bridges),
        )

    @classmethod
    def load(cls, map_name: str) -> "MapCatalogs":
        cell, level_height, wall_width, floor_thickness = load_map_geometry(map_name)
        layout = map_layout_path(map_name)
        root = read_settings_json(layout)["map"] if layout.exists() else {}
        return cls(
            {},
            {},
            wall_width,
            load_texture_catalog(map_name),
            grid_cell_size=cell,
            level_height=level_height,
            floor_thickness=floor_thickness,
            pickup_types=pickup_types(load_map_settings(map_name), str(map_settings_path(map_name))),
        ).for_layout(root)

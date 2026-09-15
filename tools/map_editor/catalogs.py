"""The catalogs the editor reads: the map registry, each map's layout kinds
and settings, and the global actor and wall-light kinds. Catalog entries are
metadata; no texture image is ever loaded."""

from __future__ import annotations

import json
import re
from math import isfinite
from dataclasses import dataclass, field, replace
from pathlib import Path

from .constants import ASSETS_PATH, GAMEPLAY_PATH, MAP_NAME_RE, MAPS_DIR


def load_wall_light_kinds() -> list[str]:
    with ASSETS_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["wall_lights"])


def load_actor_kinds() -> list[str]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        return sorted(json.load(handle)["actors"]["kinds"])


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
    entries = root.get("switch_kinds", [])
    return (
        [entry for entry in entries if isinstance(entry, dict) and isinstance(entry.get("id"), str)]
        if isinstance(entries, list)
        else []
    )


def plate_colors(root: dict, barriers: dict[str, str], bridges: dict[str, str]) -> dict[str, str]:
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
        color = entry.get("plate_color")
        return color if isinstance(color, str) and HEX_COLOR.fullmatch(color) else None

    return {
        entry["id"]: override(entry)
        or next(
            (color for switch, color in targets if switch == entry["id"] and color),
            default_color,
        )
        for entry in switch_entries(root)
    }


# The cell size and level height in metres, and one wall width in cells,
# the unit a nested map's nudge is drawn in.
def load_map_geometry(map_name: str) -> tuple[float, float, float]:
    geometry = load_map_settings(map_name)["geometry"]
    cell, level_height, wall = (float(geometry[key]) for key in ("grid_cell_size", "level_height", "wall_thickness"))
    return cell, level_height, wall / cell


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


def load_map_settings(name: str) -> dict:
    if name not in list_map_names():
        raise ValueError(f"Map {name!r} is not registered in {GAMEPLAY_PATH}.")
    return read_settings_json(map_settings_path(name))


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
    plate_colors: dict[str, str] = field(default_factory=dict)
    grid_cell_size: float = 0.0
    level_height: float = 0.0

    # The layout owns the kinds and switches; the rest stays as loaded.
    def for_layout(self, root: dict) -> "MapCatalogs":
        barriers = kind_colors(root, "barrier_kinds")
        bridges = kind_colors(root, "bridge_kinds")
        return replace(
            self,
            barrier_kind_colors=barriers,
            bridge_kind_colors=bridges,
            switches=[entry["id"] for entry in switch_entries(root)],
            plate_colors=plate_colors(root, barriers, bridges),
        )

    @classmethod
    def load(cls, map_name: str) -> "MapCatalogs":
        cell, level_height, wall_width = load_map_geometry(map_name)
        layout = map_layout_path(map_name)
        root = read_settings_json(layout)["map"] if layout.exists() else {}
        return cls(
            {},
            {},
            wall_width,
            load_texture_catalog(map_name),
            grid_cell_size=cell,
            level_height=level_height,
        ).for_layout(root)

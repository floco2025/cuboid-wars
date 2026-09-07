"""Shared constants and config tables for the map editor."""

from __future__ import annotations

import json
import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# Each registered map has a folder with layout.json and settings.json.
MAPS_DIR = REPO_ROOT / "config" / "server" / "maps"
GAMEPLAY_PATH = REPO_ROOT / "config" / "server" / "gameplay.json"


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


def load_map_bridge_kinds(map_name: str) -> dict[str, str]:
    return load_map_kinds(map_name, "bridge_kinds")


def load_map_wall_width_cells(map_name: str) -> float:
    """One wall width in cells, the unit a nested map's nudge is drawn in."""
    map_settings = load_map_settings(map_name)
    geometry = map_settings["geometry"]
    return float(geometry["wall_thickness"]) / float(geometry["grid_cell_size"])


# Editor-only: the game renders every plate alike, so this colour exists just
# to tell firework plates from barrier plates on the canvas.
FIREWORK_PLATE_COLOR = "#e040fb"

# Pressure plate `type` values; barrier and bridge plates also carry a `kind`.
PLATE_TYPE_BARRIER = "barrier"
PLATE_TYPE_BRIDGE = "bridge"
PLATE_TYPE_FIREWORK = "firework"
PLATE_TYPES = (PLATE_TYPE_BARRIER, PLATE_TYPE_BRIDGE, PLATE_TYPE_FIREWORK)

MODE_SELECT = "Select Tiles"
MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Blocked Floor"
MODE_ERASE_FLOORS = "Erase Floors"
MODE_GRASS = "Grass"
MODE_ERASE_GRASS = "Erase Grass"
MODE_ACTOR_SPAWN_ZONE = "Actor Spawn Zone"
MODE_PLAYER_SPAWN_ZONE = "Player Spawn Zone"
MODE_ERASE_SPAWN_ZONES = "Erase Spawn Zones"
MODE_ITEM = "Item"
MODE_ERASE_ITEMS = "Erase Items"
MODE_WALL = "Wall"
MODE_ERASE_WALLS = "Erase Walls"
MODE_EQUIPMENT_ERASER = "Equipment Eraser"
MODE_ERASE_EQUIPMENT_ERASERS = "Erase Equipment Erasers"
EQUIPMENT_ERASER_COLOR = "#bb88ff"
MODE_BARRIER = "Barrier"
MODE_ERASE_BARRIERS = "Erase Barriers"
MODE_LIGHT_BRIDGE = "Light Bridge"
MODE_ERASE_LIGHT_BRIDGES = "Erase Light Bridges"
MODE_RAMP_UP = "Ramp (Up)"
MODE_RAMP_DOWN = "Ramp (Down)"
MODE_ERASE_RAMPS = "Erase Ramps"
MODE_NESTED_MAP = "Nested Map"
MODE_ERASE_NESTED_MAPS = "Erase Nested Maps"
MODE_ERASE = "Erase"
MODE_ERASE_KEEP_FLOORS = "Erase (Keep Floors)"
MODE_FLOOR_MATERIAL = "Floor Material"
MODE_WALL_MATERIAL = "Wall Material"
MODE_RAMP_MATERIAL = "Ramp Material"
MODE_LIGHT = "Light"
MODE_ERASE_LIGHTS = "Erase Lights"
MODE_LADDER = "Ladder"
MODE_ERASE_LADDERS = "Erase Ladders"
MODE_PRESSURE_PLATE = "Barrier Plate"
MODE_BRIDGE_PLATE = "Bridge Plate"
MODE_FIREWORK_PLATE = "Firework Plate"
MODE_ERASE_PRESSURE_PLATES = "Erase Pressure Plates"
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
ERASE_MODES = (MODE_ERASE, MODE_ERASE_KEEP_FLOORS)
SPAWN_ZONE_MODES = (MODE_ACTOR_SPAWN_ZONE, MODE_PLAYER_SPAWN_ZONE)
MATERIAL_MODES = (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL)
FLOOR_HIT_KINDS = (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_LIGHT_BRIDGE, MODE_NESTED_MAP)
LIGHT_SIDES = ("N", "S", "E", "W")
LADDER_SIDES = LIGHT_SIDES

# Item type ids mirror `ItemType::from_config_id` in common/src/types/items.rs,
# plus "key" (which additionally carries a barrier kind).
ITEM_KEY_TYPE = "key"
ITEM_TYPES = ("single_shot", "multi_shot", "missile_pack", "portal_gun", "health_potion", "speed", "low_gravity", "gold", ITEM_KEY_TYPE)
# Canvas glyph colors for non-key items; keys use BARRIER_KIND_COLORS[kind].
# Mirror the in-game `ITEM_*_COLOR` constants in client/src/constants.rs

ITEM_TYPE_COLORS = {
    "single_shot": "#ffffff",
    "multi_shot": "#ffffff",
    "missile_pack": "#f27319",
    "portal_gun": "#338cff",
    "health_potion": "#33f24d",
    "speed": "#ffd926",
    "low_gravity": "#ffffff",
    "gold": "#ffb81f",
}
# Modes grouped by category for the mode picker. Each tuple is
# `(category label, ordered list of modes)`. The label is shown as a
# disabled separator row in the dropdown so the user sees the taxonomy
# instead of one flat list. Every element group is one map list and ends
# with its own `Erase <group>`, which clears only that element inside a
# dragged rectangle; the Erase group holds the two cross-element tools.
MODE_CATEGORIES: list[tuple[str, list[str]]] = [
    ("Floors", [MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_ERASE_FLOORS]),
    ("Grass", [MODE_GRASS, MODE_ERASE_GRASS]),
    (
        "Spawn Zones",
        [MODE_ACTOR_SPAWN_ZONE, MODE_PLAYER_SPAWN_ZONE, MODE_ERASE_SPAWN_ZONES],
    ),
    ("Walls", [MODE_WALL, MODE_ERASE_WALLS]),
    ("Barriers", [MODE_BARRIER, MODE_ERASE_BARRIERS]),
    ("Equipment Erasers", [MODE_EQUIPMENT_ERASER, MODE_ERASE_EQUIPMENT_ERASERS]),
    ("Light Bridges", [MODE_LIGHT_BRIDGE, MODE_ERASE_LIGHT_BRIDGES]),
    ("Ramps", [MODE_RAMP_UP, MODE_RAMP_DOWN, MODE_ERASE_RAMPS]),
    ("Nested Maps", [MODE_NESTED_MAP, MODE_ERASE_NESTED_MAPS]),
    ("Ladders", [MODE_LADDER, MODE_ERASE_LADDERS]),
    ("Materials", [MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL]),
    ("Lights", [MODE_LIGHT, MODE_ERASE_LIGHTS]),
    ("Pressure Plates", [MODE_PRESSURE_PLATE, MODE_BRIDGE_PLATE, MODE_FIREWORK_PLATE, MODE_ERASE_PRESSURE_PLATES]),
    ("Items", [MODE_ITEM, MODE_ERASE_ITEMS]),
    ("Erase", [MODE_ERASE, MODE_ERASE_KEEP_FLOORS]),
]

# Flat list of every mode in display order: selection first, then the
# categories, so the two never drift apart; if you add a mode, add it to
# its category.
MODES: list[str] = [MODE_SELECT, *(mode for _, group in MODE_CATEGORIES for mode in group)]

# Named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
PLAYER_ZONE_LIST = "player_spawn_zones"
SPAWN_ZONE_LISTS = (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST)
ITEMS_LIST = "items"
NESTED_MAPS_LIST = "nested_maps"

# Same rule the server enforces on map names: they become file names.
MAP_NAME_RE = re.compile(r"^[A-Za-z0-9_-]+$")


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


DEFAULT_ACTOR_COUNT = 1
SPAWN_ZONE_HANDLE_PIXELS = 8.0
STATUS_TIMEOUT_MS = 4000

EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20
FACES = ("top", "bottom", "north", "south", "east", "west")

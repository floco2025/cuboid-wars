"""Shared constants and config tables for the map editor."""

from __future__ import annotations

import json
import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# One map JSON per named map; the editor's CLI argument is the map name.
MAPS_DIR = REPO_ROOT / "config" / "server" / "maps"
GAMEPLAY_PATH = REPO_ROOT / "config" / "server" / "gameplay.json"


def _load_material_aliases() -> set[str]:
    assets_path = REPO_ROOT / "config" / "client" / "assets.json"
    with assets_path.open("r", encoding="utf-8") as handle:
        assets = json.load(handle)
    return set(assets.get("aliases", {}).keys())


MATERIAL_ALIASES = _load_material_aliases()

HEX_COLOR = re.compile(r"#[0-9a-fA-F]{6}")


# A map's kind catalog from its gameplay settings, in catalog order: id → "#rrggbb".
def load_map_kinds(map_name: str, key: str) -> dict[str, str]:
    with GAMEPLAY_PATH.open("r", encoding="utf-8") as handle:
        gameplay = json.load(handle)
    map_settings = gameplay.get("maps", {}).get(map_name)
    if map_settings is None:
        return {}
    if key not in map_settings:
        raise ValueError(f"maps.{map_name}.{key} is required; use [] when the map has none")
    value = map_settings[key]
    if not isinstance(value, list):
        raise ValueError(f"maps.{map_name}.{key} must be an array of {{id, color}} objects")
    kinds: dict[str, str] = {}
    for idx, entry in enumerate(value):
        path = f"maps.{map_name}.{key}[{idx}]"
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

# Editor-only: the game renders every plate alike, so this colour exists just
# to tell firework plates from barrier plates on the canvas.
FIREWORK_PLATE_COLOR = "#e040fb"

# Pressure plate `type` values; barrier and bridge plates also carry a `kind`.
PLATE_TYPE_BARRIER = "barrier"
PLATE_TYPE_BRIDGE = "bridge"
PLATE_TYPE_FIREWORK = "firework"
PLATE_TYPES = (PLATE_TYPE_BARRIER, PLATE_TYPE_BRIDGE, PLATE_TYPE_FIREWORK)

# Stable fallback alias used when a segment has no material data, when a new
# segment is created without an explicit choice, or when we need *some* legal
# value to satisfy the alias validator. Derived from the loaded catalog so it
# can't drift to a non-existent alias if `assets.json` changes — sorted-first
# for determinism. Empty string when the catalog defines no aliases at all
# (validator skips the check in that case, so an empty value is harmless).
DEFAULT_ALIAS: str = next(iter(sorted(MATERIAL_ALIASES)), "")

MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Blocked Floor"
MODE_ERASE_FLOORS = "Erase Floors"
MODE_GRASS = "Grass"
MODE_ERASE_GRASS = "Erase Grass"
MODE_ACTOR_SPAWN_PAINT = "Actor Spawn Zone (Paint)"
MODE_PLAYER_SPAWN_PAINT = "Player Spawn Zone (Paint)"
MODE_SPAWN_ZONE_EDIT = "Spawn Zone (Edit)"
MODE_ERASE_SPAWN_ZONES = "Erase Spawn Zones"
MODE_ITEM = "Item"
MODE_ERASE_ITEMS = "Erase Items"
MODE_WALL = "Wall"
MODE_ERASE_WALLS = "Erase Walls"
MODE_BARRIER = "Barrier"
MODE_ERASE_BARRIERS = "Erase Barriers"
MODE_LIGHT_BRIDGE = "Light Bridge"
MODE_ERASE_LIGHT_BRIDGES = "Erase Light Bridges"
MODE_RAMP_UP = "Ramp (Up)"
MODE_RAMP_DOWN = "Ramp (Down)"
MODE_ERASE_RAMPS = "Erase Ramps"
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
SPAWN_PAINT_MODES = (MODE_ACTOR_SPAWN_PAINT, MODE_PLAYER_SPAWN_PAINT)
MATERIAL_MODES = (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL)
FLOOR_HIT_KINDS = (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_LIGHT_BRIDGE)
LIGHT_SIDES = ("N", "S", "E", "W")
LADDER_SIDES = LIGHT_SIDES

# Item type ids mirror `ItemType::from_config_id` in common/src/types/map.rs,
# plus "key" (which additionally carries a barrier kind).
ITEM_KEY_TYPE = "key"
ITEM_TYPES = ("speed", "multi_shot", "low_gravity", "health_potion", "cookie", "missile_pack", ITEM_KEY_TYPE)
# Canvas glyph colors for non-key items; keys use BARRIER_KIND_COLORS[kind].
# Mirror the in-game `ITEM_*_COLOR` constants in client/src/constants.rs
# (cookie renders from its gold texture in-game, so it keeps the gold the
# old cookie-zone overlay used).
ITEM_TYPE_COLORS = {
    "speed": "#ffd926",
    "multi_shot": "#ff4040",
    "low_gravity": "#4dd9ff",
    "health_potion": "#33f24d",
    "cookie": "#facc15",
    "missile_pack": "#f27319",
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
        [
            MODE_ACTOR_SPAWN_PAINT,
            MODE_PLAYER_SPAWN_PAINT,
            MODE_SPAWN_ZONE_EDIT,
            MODE_ERASE_SPAWN_ZONES,
        ],
    ),
    ("Walls", [MODE_WALL, MODE_ERASE_WALLS]),
    ("Barriers", [MODE_BARRIER, MODE_ERASE_BARRIERS]),
    ("Light Bridges", [MODE_LIGHT_BRIDGE, MODE_ERASE_LIGHT_BRIDGES]),
    ("Ramps", [MODE_RAMP_UP, MODE_RAMP_DOWN, MODE_ERASE_RAMPS]),
    ("Ladders", [MODE_LADDER, MODE_ERASE_LADDERS]),
    ("Materials", [MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL]),
    ("Lights", [MODE_LIGHT, MODE_ERASE_LIGHTS]),
    ("Pressure Plates", [MODE_PRESSURE_PLATE, MODE_BRIDGE_PLATE, MODE_FIREWORK_PLATE, MODE_ERASE_PRESSURE_PLATES]),
    ("Items", [MODE_ITEM, MODE_ERASE_ITEMS]),
    ("Erase", [MODE_ERASE, MODE_ERASE_KEEP_FLOORS]),
]

# Flat list of every mode in display order. Derived from `MODE_CATEGORIES`
# so the two never drift apart; if you add a mode, add it to its category.
MODES: list[str] = [mode for _, group in MODE_CATEGORIES for mode in group]

# Named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
PLAYER_ZONE_LIST = "player_spawn_zones"
SPAWN_ZONE_LISTS = (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST)
ITEMS_LIST = "items"

DEFAULT_ACTOR_COUNT = 1
SPAWN_ZONE_HANDLE_PIXELS = 8.0
STATUS_TIMEOUT_MS = 4000



MIN_CELL = 12.0
EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20
FACES = ("top", "bottom", "north", "south", "east", "west")

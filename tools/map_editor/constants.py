"""Shared constants and config tables for the map editor."""

from __future__ import annotations

import json
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# One map JSON per named map; the editor's CLI argument is the map name.
MAPS_DIR = REPO_ROOT / "config" / "server" / "maps"
SUPPORTED_VERSION = 1


def _load_shared_configs() -> tuple[list[str], dict[str, str], set[str]]:
    gameplay_path = REPO_ROOT / "config" / "common" / "gameplay.json"
    assets_path = REPO_ROOT / "config" / "client" / "assets.json"
    with gameplay_path.open("r", encoding="utf-8") as handle:
        gameplay = json.load(handle)
    with assets_path.open("r", encoding="utf-8") as handle:
        assets = json.load(handle)
    ids: list[str] = list(gameplay.get("barrier_kinds", []))
    colors: dict[str, str] = dict(assets.get("barrier_kind_colors", {}))
    aliases: set[str] = set(assets.get("aliases", {}).keys())
    for id_ in ids:
        if id_ not in colors:
            raise RuntimeError(
                f"barrier kind {id_!r} has no color in assets.json `barrier_kind_colors`; "
                "add an entry or remove the id from gameplay.json"
            )
    return ids, colors, aliases


BARRIER_KIND_TABLE, BARRIER_KIND_COLORS, MATERIAL_ALIASES = _load_shared_configs()

# Stable fallback alias used when a segment has no material data, when a new
# segment is created without an explicit choice, or when we need *some* legal
# value to satisfy the alias validator. Derived from the loaded catalog so it
# can't drift to a non-existent alias if `assets.json` changes — sorted-first
# for determinism. Empty string when the catalog defines no aliases at all
# (validator skips the check in that case, so an empty value is harmless).
DEFAULT_ALIAS: str = next(iter(sorted(MATERIAL_ALIASES)), "")

MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Blocked Floor"
MODE_GRASS = "Grass"
MODE_ERASE_GRASS = "Erase Grass"
MODE_ACTOR_SPAWN_PAINT = "Actor Spawn Zone (Paint)"
MODE_PLAYER_SPAWN_PAINT = "Player Spawn Zone (Paint)"
MODE_SPAWN_ZONE_EDIT = "Spawn Zone (Edit)"
MODE_ITEM = "Item"
MODE_ERASE_ITEMS = "Erase Items"
MODE_WALL = "Wall"
MODE_BARRIER = "Barrier"
MODE_RAMP_UP = "Ramp (Up)"
MODE_RAMP_DOWN = "Ramp (Down)"
MODE_ERASE = "Erase"
MODE_ERASE_KEEP_FLOORS = "Erase (Keep Floors)"
MODE_FLOOR_MATERIAL = "Floor Material"
MODE_WALL_MATERIAL = "Wall Material"
MODE_RAMP_MATERIAL = "Ramp Material"
MODE_LIGHT = "Light"
MODE_ERASE_LIGHTS = "Erase Lights"
MODE_PRESSURE_PLATE = "Pressure Plate"
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
ERASE_MODES = (MODE_ERASE, MODE_ERASE_KEEP_FLOORS)
SPAWN_PAINT_MODES = (MODE_ACTOR_SPAWN_PAINT, MODE_PLAYER_SPAWN_PAINT)
MATERIAL_MODES = (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL)
FLOOR_HIT_KINDS = (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR)
LIGHT_SIDES = ("N", "S", "E", "W")

# Item type ids mirror `ItemType::from_config_id` in common/src/types/map.rs,
# plus "key" (which additionally carries a barrier kind).
ITEM_KEY_TYPE = "key"
ITEM_TYPES = ("speed", "multi_shot", "phasing", "low_gravity", "health_potion", "cookie", "missile_pack", ITEM_KEY_TYPE)
# Canvas glyph colors for non-key items; keys use BARRIER_KIND_COLORS[kind].
# Mirror the in-game `ITEM_*_COLOR` constants in client/src/constants.rs
# (cookie renders from its gold texture in-game, so it keeps the gold the
# old cookie-zone overlay used).
ITEM_TYPE_COLORS = {
    "speed": "#ffd926",
    "multi_shot": "#ff4040",
    "phasing": "#33ff33",
    "low_gravity": "#4dd9ff",
    "health_potion": "#33f24d",
    "cookie": "#facc15",
    "missile_pack": "#f27319",
}
# Modes grouped by category for the mode picker. Each tuple is
# `(category label, ordered list of modes)`. The label is shown as a
# disabled separator row in the dropdown so the user sees the taxonomy
# instead of one flat list.
MODE_CATEGORIES: list[tuple[str, list[str]]] = [
    ("Floors", [MODE_FLOOR, MODE_INACCESSIBLE_FLOOR]),
    ("Grass", [MODE_GRASS, MODE_ERASE_GRASS]),
    (
        "Spawn Zones",
        [
            MODE_ACTOR_SPAWN_PAINT,
            MODE_PLAYER_SPAWN_PAINT,
            MODE_SPAWN_ZONE_EDIT,
        ],
    ),
    ("Walls + Barriers", [MODE_WALL, MODE_BARRIER]),
    ("Ramps", [MODE_RAMP_UP, MODE_RAMP_DOWN]),
    ("Materials", [MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL]),
    ("Lights", [MODE_LIGHT, MODE_ERASE_LIGHTS]),
    ("Pressure Plates", [MODE_PRESSURE_PLATE]),
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

"""Shared constants and config tables for the map editor."""

from __future__ import annotations

import json
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MAP = REPO_ROOT / "config" / "server" / "map.json"
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

MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Inaccessible Floor"
MODE_ACTOR_SPAWN_PAINT = "Actor Spawn Zone (Paint)"
MODE_PLAYER_SPAWN_PAINT = "Player Spawn Zone (Paint)"
MODE_COOKIE_SPAWN_PAINT = "Cookie Spawn Zone (Paint)"
MODE_KEY_SPAWN_PAINT = "Key Spawn Zone (Paint)"
MODE_SPAWN_ZONE_EDIT = "Spawn Zone (Edit)"
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
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
ERASE_MODES = (MODE_ERASE, MODE_ERASE_KEEP_FLOORS)
SPAWN_PAINT_MODES = (MODE_ACTOR_SPAWN_PAINT, MODE_PLAYER_SPAWN_PAINT, MODE_COOKIE_SPAWN_PAINT, MODE_KEY_SPAWN_PAINT)
MATERIAL_MODES = (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL)
FLOOR_HIT_KINDS = ("Floor", "Inaccessible Floor")
LIGHT_SIDES = ("N", "S", "E", "W")
MODES = [
    MODE_FLOOR,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ACTOR_SPAWN_PAINT,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_COOKIE_SPAWN_PAINT,
    MODE_KEY_SPAWN_PAINT,
    MODE_SPAWN_ZONE_EDIT,
    MODE_WALL,
    MODE_BARRIER,
    MODE_RAMP_UP,
    MODE_RAMP_DOWN,
    MODE_ERASE,
    MODE_ERASE_KEEP_FLOORS,
    MODE_FLOOR_MATERIAL,
    MODE_WALL_MATERIAL,
    MODE_RAMP_MATERIAL,
    MODE_LIGHT,
    MODE_ERASE_LIGHTS,
]

# Two named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
PLAYER_ZONE_LIST = "player_spawn_zones"
COOKIE_ZONE_LIST = "cookie_spawn_zones"
KEY_ZONE_LIST = "key_spawn_zones"
SPAWN_ZONE_LISTS = (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST, COOKIE_ZONE_LIST, KEY_ZONE_LIST)

DEFAULT_ACTOR_COUNT = 1
SPAWN_ZONE_HANDLE_PIXELS = 8.0
STATUS_TIMEOUT_MS = 4000

# Body for the Help → Tool Reference dialog. Add a new entry as `(tool name,
# one-line description)`; rendered as "tool name: description" lines.
TOOL_REFERENCE_ENTRIES: list[tuple[str, str]] = [
    ("Floor", "drag cells to add floor."),
    (
        "Inaccessible Floor",
        "drag cells to add floor slabs that never spawn items, players, or lights.",
    ),
    ("Actor Spawn Zone (Paint)", "drag a rectangle, then enter Kind and Count."),
    ("Player Spawn Zone (Paint)", "drag a rectangle. No prompt — players spawn anywhere in any player zone."),
    ("Cookie Spawn Zone (Paint)", "drag a rectangle. Cookies only spawn on walkable floors inside one of these zones."),
    (
        "Key Spawn Zone (Paint)",
        "drag a rectangle, then pick a kind from the dialog. One key of that kind "
        "spawns at the first eligible cell of the zone and respawns after collection.",
    ),
    (
        "Spawn Zone (Edit)",
        "click a zone to select; drag the body to move, drag a corner/edge handle to "
        "resize. Right-click to edit fields (actor zones only) or delete.",
    ),
    ("Wall", "drag along grid lines to place atomic wall edges."),
    (
        "Barrier",
        "drag along grid lines to place a translucent pulsating force-field; a dialog "
        "asks which kind to use. Kinds and colors are defined in "
        "`config/common/gameplay.json::barrier_kinds` + "
        "`config/client/assets.json::barrier_kind_colors`.",
    ),
    ("Ramp (Up)", "drag from this level toward the upper level."),
    ("Ramp (Down)", "drag from this level toward the lower level."),
    ("Erase", "click an item, drag cells to erase an area, or right-click for the context menu."),
    ("Erase (Keep Floors)", "erase walls, ramps, and spawn zones while preserving floor and inaccessible floor cells."),
    (
        "Light",
        "click a cell near a wall to add a wall light on that side; click an existing "
        "light marker to remove it. Use Edit → Auto-Place Lights to fill the current "
        "level on a stride; Edit → Clear Lights On Level to start over.",
    ),
    ("Erase Lights", "drag a rectangle to remove every light inside it on the current level."),
]


MIN_CELL = 12.0
EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20
FACES = ("top", "bottom", "north", "south", "east", "west")

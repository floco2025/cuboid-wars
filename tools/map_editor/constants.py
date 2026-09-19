"""Shared constants for the map editor."""

from __future__ import annotations

import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# Each registered map has a folder with layout.json and settings.json.
MAPS_DIR = REPO_ROOT / "config" / "server" / "maps"
GAMEPLAY_PATH = REPO_ROOT / "config" / "server" / "gameplay.json"
ASSETS_PATH = REPO_ROOT / "config" / "client" / "assets.json"


# Editor-only: the game renders every plate alike, so these colours exist
# just to tell the switches apart on the canvas, by catalog position.
UNKNOWN_SWITCH_PLATE_COLOR = "#9ca3af"

MODE_SELECT = "Select"
MODE_SAMPLE = "Sample Tool"
MODE_JUMP_REACH = "Jump Reach"
MODE_PORTAL_JUMP = "Portal Jump"
MODE_RUN_TIME = "Run Time"
MODE_FLOOR = "Floor"
MODE_INACCESSIBLE_FLOOR = "Blocked Floor"
MODE_ERASE_FLOORS = "Erase Floors"
MODE_TERRAIN = "Terrain"
MODE_ERASE_TERRAIN = "Erase Terrain"
MODE_ACTOR_SPAWN_ZONE = "Actor Spawn Zone"
MODE_CHECKPOINT = "Checkpoint"
MODE_ERASE_CHECKPOINTS = "Erase Checkpoints"
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
MODE_PRESSURE_PLATE = "Pressure Plate"
MODE_ERASE_PRESSURE_PLATES = "Erase Pressure Plates"
RAMP_MODES = (MODE_RAMP_UP, MODE_RAMP_DOWN)
ERASE_MODES = (MODE_ERASE, MODE_ERASE_KEEP_FLOORS)
ZONE_MODES = (MODE_ACTOR_SPAWN_ZONE, MODE_CHECKPOINT)
MATERIAL_MODES = (MODE_FLOOR_MATERIAL, MODE_WALL_MATERIAL, MODE_RAMP_MATERIAL)
# What a pick under the cursor found; the kind doubles as the hover title
# and the "Erase <kind>" label.
HIT_FLOOR = MODE_FLOOR
HIT_INACCESSIBLE_FLOOR = MODE_INACCESSIBLE_FLOOR
HIT_TERRAIN = MODE_TERRAIN
HIT_LIGHT_BRIDGE = MODE_LIGHT_BRIDGE
HIT_NESTED_MAP = MODE_NESTED_MAP
HIT_EQUIPMENT_ERASER = MODE_EQUIPMENT_ERASER
HIT_WALL = "Wall"
HIT_BARRIER = "Barrier"
HIT_LIGHT = "Light"
HIT_LADDER = "Ladder"
HIT_ITEM = "Item"
HIT_RAMP = "Ramp"
HIT_SPAWN_ZONE = "Spawn Zone"
HIT_CHECKPOINT = MODE_CHECKPOINT
HIT_PRESSURE_PLATE = "Pressure Plate"
# The picks Erase (Keep Floors) leaves in place.
FLOOR_HIT_KINDS = (HIT_FLOOR, HIT_INACCESSIBLE_FLOOR, HIT_TERRAIN, HIT_LIGHT_BRIDGE, HIT_NESTED_MAP)
LIGHT_SIDES = ("N", "S", "E", "W")
LADDER_SIDES = LIGHT_SIDES

# Item type ids mirror `ItemType::from_config_id` in common/src/types/items.rs,
# plus "key" (which additionally carries a barrier kind).
POWER_UP_TYPES = ("single_shot", "multi_shot", "portal_gun", "speed", "low_gravity")
ITEM_KEY_TYPE = "key"
ITEM_TYPES = (
    "single_shot",
    "multi_shot",
    "missile_pack",
    "portal_gun",
    "health_potion",
    "speed",
    "low_gravity",
    "gold",
    ITEM_KEY_TYPE,
)
# Canvas glyph colors for non-key items, mirroring the in-game `ITEM_*_COLOR`
# constants in client/src/constants.rs; keys use their barrier kind's color.
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
# Named lists in map_data so the editor can refer to them generically.
ACTOR_ZONE_LIST = "actor_spawn_zones"
CHECKPOINT_LIST = "checkpoints"
# The course starts at checkpoint 0: every player begins there, and the game
# neither marks nor announces it. Its type is meaningless, and the file
# carries this one for it.
START_CHECKPOINT = 0
START_CHECKPOINT_TYPE = "individual"
CHECKPOINT_TYPE_LABELS = {"individual": "Individual", "group_any": "Group — any", "group_all": "Group — all"}
# What an actor zone does once any player has reached its `until_checkpoint`.
CHECKPOINT_RESPONSE_LABELS = {"stop": "Stop spawning", "destroy": "Self-destruct actors"}
# What `switch_inverted` selects: a barrier or bridge names when the field is
# solid, a zone or a nested map when it responds.
SOLID_WHEN = "Solid when"
RESPOND_WHEN = "Respond when"
ZONE_LISTS = (ACTOR_ZONE_LIST, CHECKPOINT_LIST)
# Which zone a pick under the cursor prefers when zones overlap a cell: a
# checkpoint first, then an actor zone.
ZONE_PICK_ORDER = (CHECKPOINT_LIST, ACTOR_ZONE_LIST)
ITEMS_LIST = "items"
NESTED_MAPS_LIST = "nested_maps"

# Same rule the server enforces on map names: they become file names.
MAP_NAME_RE = re.compile(r"^[A-Za-z0-9_-]+$")

DEFAULT_ACTOR_COUNT = 1
DEFAULT_ACTOR_RESPAWN_SECS = 90
DEFAULT_ACTOR_BEAM_IN_SECS = 3.0
SPAWN_ZONE_HANDLE_PIXELS = 8.0
# Screen distance within which a click picks a wall, barrier, eraser, or ladder edge.
EDGE_PICK_PIXELS = 6.0
STATUS_TIMEOUT_MS = 4000

EDITOR_CELL = 36
DEFAULT_GRID_COLS = 20
DEFAULT_GRID_ROWS = 20
FACES = ("top", "bottom", "north", "south", "east", "west")
TERRAIN_FACES = ("bottom", "north", "south", "east", "west")

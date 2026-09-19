"""Stable palette order and the existing canvas modes behind each tool."""

from dataclasses import dataclass

from . import constants as c


@dataclass(frozen=True)
class Tool:
    mode: str
    label: str
    erase: str | None = None


PINNED_TOOLS = (Tool(c.MODE_SELECT, "Select"), Tool(c.MODE_SAMPLE, "Sample"), Tool(c.MODE_ERASE, "Erase"))
TOOL_GROUPS = (
    (
        "Build",
        (
            Tool(c.MODE_FLOOR, "Floor", c.MODE_ERASE_FLOORS),
            Tool(c.MODE_INACCESSIBLE_FLOOR, "Blocked floor", c.MODE_ERASE_FLOORS),
            Tool(c.MODE_TERRAIN, "Terrain", c.MODE_ERASE_TERRAIN),
            Tool(c.MODE_WALL, "Wall", c.MODE_ERASE_WALLS),
            Tool(c.MODE_RAMP, "Ramp", c.MODE_ERASE_RAMPS),
            Tool(c.MODE_LADDER, "Ladder", c.MODE_ERASE_LADDERS),
            Tool(c.MODE_NESTED_MAP, "Nested map", c.MODE_ERASE_NESTED_MAPS),
        ),
    ),
    (
        "Zones & Items",
        (
            Tool(c.MODE_ACTOR_SPAWN_ZONE, "Actor zone", c.MODE_ERASE_SPAWN_ZONES),
            Tool(c.MODE_CHECKPOINT, "Checkpoint", c.MODE_ERASE_CHECKPOINTS),
            Tool(c.MODE_ITEM, "Item", c.MODE_ERASE_ITEMS),
        ),
    ),
    (
        "Mechanisms",
        (
            Tool(c.MODE_BARRIER, "Barrier", c.MODE_ERASE_BARRIERS),
            Tool(c.MODE_EQUIPMENT_ERASER, "Equip. eraser", c.MODE_ERASE_EQUIPMENT_ERASERS),
            Tool(c.MODE_LIGHT_BRIDGE, "Light bridge", c.MODE_ERASE_LIGHT_BRIDGES),
            Tool(c.MODE_PRESSURE_PLATE, "Pressure plate", c.MODE_ERASE_PRESSURE_PLATES),
        ),
    ),
    (
        "Appearance",
        (
            Tool(c.MODE_FLOOR_MATERIAL, "Floor material"),
            Tool(c.MODE_WALL_MATERIAL, "Wall material"),
            Tool(c.MODE_RAMP_MATERIAL, "Ramp material"),
            Tool(c.MODE_LIGHT, "Light", c.MODE_ERASE_LIGHTS),
        ),
    ),
    (
        "Measure",
        (
            Tool(c.MODE_JUMP_REACH, "Jump reach"),
            Tool(c.MODE_PORTAL_JUMP, "Portal jump"),
            Tool(c.MODE_RUN_TIME, "Run time"),
        ),
    ),
)
TOOLS = {tool.mode: tool for tool in (*PINNED_TOOLS, *(tool for _, group in TOOL_GROUPS for tool in group))}
# Shared erase modes retain the selected variant (blocked floor).
# Use the first variant when there is no such selection.
ERASE_TOOLS = {}
for tool in TOOLS.values():
    if tool.erase:
        ERASE_TOOLS.setdefault(tool.erase, tool.mode)

MODE_TO_TOOL = {mode: mode for mode in TOOLS} | ERASE_TOOLS | {c.MODE_ERASE_KEEP_FLOORS: c.MODE_ERASE}


def tool_for_mode(mode: str, previous: str = c.MODE_SELECT) -> Tool:
    tool = TOOLS[previous]
    if tool.erase == mode:
        return tool
    return TOOLS[MODE_TO_TOOL[mode]]

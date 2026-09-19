"""Editable, named properties exposed by the selection inspector."""

from dataclasses import dataclass

from .constants import (
    CHECKPOINT_RESPONSE_LABELS,
    CHECKPOINT_TYPE_LABELS,
    FACES,
    INITIAL_STATE,
    INITIAL_STATE_LABELS,
    RAMP_DIRECTIONS,
    RAMP_SHAPE_LABELS,
    TERRAIN_FACES,
)
from .nesting import MOTION_LABELS, MOTION_TOOLTIPS

NUDGE_TOOLTIPS = ("Wall widths across columns (X) or rows (Z)", "Floor thicknesses upward")


@dataclass(frozen=True)
class PropertyField:
    key: tuple
    label: str
    kind: str = "text"
    choices: tuple = ()
    # (value, "#rrggbb") pairs for the choices drawn with a colour swatch.
    colors: tuple = ()
    tooltip: str = ""


def fields_for(window, name):
    fields = []

    def add(key, label, kind="text", choices=(), colors=None, tooltip=""):
        fields.append(
            PropertyField(
                (key,) if isinstance(key, str) else key,
                label,
                kind,
                tuple(choices),
                tuple((colors or {}).items()),
                tooltip,
            )
        )

    def choice(key, label, values, colors=None):
        add(key, label, "choice", [(value, str(value)) for value in values], colors)

    if name == "ramps":
        add("levels", "Storeys", "positive_int")
        choice("direction", "Direction", RAMP_DIRECTIONS)
        add("shape", "Shape", "choice", RAMP_SHAPE_LABELS.items())
    if name in ("floors", "inaccessible_floors", "walls", "ramps", "terrain"):
        for face in TERRAIN_FACES if name == "terrain" else FACES:
            choice(face, face.capitalize(), window.materials_catalog)
    if name == "actor_spawn_zones":
        add(
            "level",
            "First level",
            "choice",
            [(i, level.get("name") or f"Level {i}") for i, level in enumerate(window.map_data["levels"])],
        )
    if name == "actor_spawn_zones":
        choice("kind", "Actor", window.actor_kinds)
        add("count", "Count", "counts", tooltip="Counts for one, two, three, etc. players. The last count repeats.")
        add("respawn_secs", "Respawn (s)", "respawn", tooltip="Seconds before refilling a slot; Never fills it once.")
        add(
            "beam_in_secs",
            "Beam-in (s)",
            "nonnegative",
            tooltip="Seconds the ghost shows before an actor appears; 0 pops it in at once.",
        )
        add("levels", "Levels", "positive_int")
        add("roam_distance", "Roam (m)", "nonnegative")
        add(
            "until_checkpoint",
            "Until checkpoint",
            "checkpoint",
            tooltip="The checkpoint that ends the zone once any player reaches it; Always keeps it open.",
        )
        add(
            "on_checkpoint",
            "Then",
            "choice",
            CHECKPOINT_RESPONSE_LABELS.items(),
            tooltip="Whether the zone's remaining actors self-destruct at that checkpoint.",
        )
    elif name == "checkpoints":
        add(
            "number",
            "Number",
            "nonnegative_int",
            tooltip="Orders the course; 0 is the start, and checkpoints sharing a number pool their spots.",
        )
        add("type", "Type", "choice", CHECKPOINT_TYPE_LABELS.items())
    elif name == "items":
        choice("type", "Item", window.pickup_types)
        choice("field", "Field", [None, *window.fields], window.field_colors)
    elif name in ("barriers", "light_bridges"):
        choice("field", "Field", window.fields, window.field_colors)
    elif name == "lights":
        choice("kind", "Style", window.wall_light_kinds)
        choice("side", "Side", ("N", "S", "E", "W"))
    elif name == "ladders":
        add("levels", "Storeys", "positive_int")
        choice("side", "Side", ("N", "S", "E", "W"))
    elif name == "nested_maps":
        choice("map", "Map", window.nested_map_names())
        add("motion", "Motion", "choice", MOTION_LABELS.items())
        add("travel_secs", "Travel time (s)", "positive", tooltip=MOTION_TOOLTIPS["travel_secs"])
        add("pause_secs", "Cycle pause (s)", "nonnegative", tooltip=MOTION_TOOLTIPS["pause_secs"])
        add("phase_secs", "Cycle phase (s)", "nonnegative", tooltip=MOTION_TOOLTIPS["phase_secs"])
    if name in ("actor_spawn_zones", "nested_maps", "pressure_plates"):
        values = window.switches if name == "pressure_plates" else [None, *window.switches]
        choice("switch", "Switch", values, window.switch_colors)
        if name != "pressure_plates":
            add("initially_on", INITIAL_STATE, "choice", INITIAL_STATE_LABELS.items())
    if name == "nested_maps":
        add(
            "to_level",
            "End 2 level",
            "choice",
            [(i, level.get("name") or f"Level {i}") for i, level in enumerate(window.map_data["levels"])],
        )
        for end, label in (("from_nudge", "Nudge end 1"), ("to_nudge", "Nudge end 2")):
            for axis, letter in enumerate(("X", "Y", "Z")):
                add((end, axis), f"{label} {letter}", "number", tooltip=NUDGE_TOOLTIPS[axis == 1])
    return fields


def property_value(entry, key):
    value = entry.get(key[0])
    if len(key) == 2:
        return value[key[1]]
    if key[0] in FACES:
        return entry.get(key[0], entry.get("all", ""))
    if key[0] == "initially_on":
        return entry.get(key[0], True)
    if key[0] == "levels":
        return entry.get(key[0], 1)
    if key[0] in ("roam_distance", "beam_in_secs"):
        return entry.get(key[0], 0.0)
    if key[0] == "on_checkpoint":
        return entry.get(key[0], "stop")
    return value

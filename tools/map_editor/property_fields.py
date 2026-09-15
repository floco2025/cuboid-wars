"""Editable, named properties exposed by the selection inspector."""

from dataclasses import dataclass

from .constants import CHECKPOINT_TYPE_LABELS, FACES, ITEM_TYPES, TERRAIN_FACES


@dataclass(frozen=True)
class PropertyField:
    key: tuple
    label: str
    kind: str = "text"
    choices: tuple = ()
    # (value, "#rrggbb") pairs for the choices drawn with a colour swatch.
    colors: tuple = ()


def fields_for(window, name):
    fields = []

    def add(key, label, kind="text", choices=(), colors=None):
        fields.append(
            PropertyField(
                (key,) if isinstance(key, str) else key,
                label,
                kind,
                tuple(choices),
                tuple((colors or {}).items()),
            )
        )

    def choice(key, label, values, colors=None):
        add(key, label, "choice", [(value, str(value)) for value in values], colors)

    if name in ("floors", "inaccessible_floors", "walls", "ramps", "terrain"):
        for face in TERRAIN_FACES if name == "terrain" else FACES:
            choice(face, face.capitalize(), window.materials_catalog)
    if name in ("actor_spawn_zones", "player_spawn_zones"):
        add(
            "level",
            "First level",
            "choice",
            [(i, level.get("name") or f"Level {i}") for i, level in enumerate(window.map_data["levels"])],
        )
    if name == "actor_spawn_zones":
        choice("kind", "Actor", window.actor_kinds)
        add("count", "Count", "counts")
        add("respawn_secs", "Respawn (s)", "respawn")
        add("levels", "Levels", "positive_int")
        add("roam_distance", "Roam (m)", "nonnegative")
    elif name == "player_spawn_zones":
        add("levels", "Levels", "positive_int")
    elif name == "checkpoints":
        add("type", "Type", "choice", CHECKPOINT_TYPE_LABELS.items())
    elif name == "items":
        choice("type", "Item", ITEM_TYPES)
        choice("kind", "Key kind", [None, *window.key_kinds], window.barrier_kind_colors)
    elif name == "barriers":
        choice("kind", "Kind", window.barrier_kinds, window.barrier_kind_colors)
    elif name == "light_bridges":
        choice("kind", "Kind", window.bridge_kinds, window.bridge_kind_colors)
    elif name == "lights":
        choice("kind", "Style", window.wall_light_kinds)
        choice("side", "Side", ("N", "S", "E", "W"))
    elif name == "ladders":
        add("levels", "Storeys", "positive_int")
        choice("side", "Side", ("N", "S", "E", "W"))
    elif name == "nested_maps":
        choice("map", "Geometry", window.nested_map_names())
        add(
            "to_level",
            "End level",
            "choice",
            [(i, level.get("name") or f"Level {i}") for i, level in enumerate(window.map_data["levels"])],
        )
        add("travel_secs", "Travel (s)", "positive")
        add("pause_secs", "Pause (s)", "nonnegative")
        add("phase_secs", "Phase (s)", "nonnegative")
        for end, label in (("from_nudge", "Start"), ("to_nudge", "End")):
            for axis, letter in enumerate(("X", "Y", "Z")):
                add((end, axis), f"{label} {letter}", "number")
    if name in ("barriers", "light_bridges", "actor_spawn_zones", "nested_maps", "pressure_plates"):
        values = window.switches if name == "pressure_plates" else [None, *window.switches]
        choice("switch", "Plate kind", values, window.plate_colors)
        if name != "pressure_plates":
            add("switch_inverted", "Respond when", "choice", [(False, "On"), (True, "Off")])
    return fields


def property_value(entry, key):
    value = entry.get(key[0])
    if len(key) == 2:
        return value[key[1]]
    if key[0] in FACES:
        return entry.get(key[0], entry.get("all", ""))
    if key[0] == "switch_inverted":
        return entry.get(key[0], False)
    if key[0] == "levels":
        return entry.get(key[0], 1)
    if key[0] == "roam_distance":
        return entry.get(key[0], 0.0)
    return value

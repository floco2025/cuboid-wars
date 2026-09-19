"""Descriptions of the map element picked under the cursor."""

from __future__ import annotations

from .spawn_counts import actor_count_preview

from .constants import (
    ACTOR_ZONE_LIST,
    HIT_BARRIER,
    HIT_FLOOR,
    HIT_INACCESSIBLE_FLOOR,
    HIT_ITEM,
    HIT_LADDER,
    HIT_LIGHT,
    HIT_LIGHT_BRIDGE,
    HIT_NESTED_MAP,
    HIT_PRESSURE_PLATE,
    HIT_RAMP,
    HIT_TERRAIN,
    HIT_CHECKPOINT,
    CHECKPOINT_TYPE_LABELS,
    HIT_SPAWN_ZONE,
    INITIAL_STATE,
    INITIAL_STATE_LABELS,
    HIT_WALL,
    ITEMS_LIST,
    NESTED_MAPS_LIST,
)
from .checkpoint_numbers import is_start
from .display import materials_summary, pressure_plate_label
from .nesting import MOTION_LABELS, motion_uses_cycle
from .geometry import ramp_key
from .normalization import edge_key, ladder_key, nested_map_key

SIDE_LABELS = {"N": "North", "S": "South", "E": "East", "W": "West"}


# A target that starts on with no switch has no controls to show.
def with_controls(label: str, target: dict) -> str:
    controls = [f"Switch: {target['switch']}"] if target.get("switch") else []
    if controls or target.get("initially_on") is False:
        controls.append(f"{INITIAL_STATE}: {INITIAL_STATE_LABELS[target.get('initially_on') is not False]}")
    return label + "\n" + " · ".join(controls) if controls else label


# `fields` holds the root layout's field entries by name.
def element_hover_text(data: dict, level_idx: int, hit, fields: dict[str, dict] | None = None) -> str | None:
    if hit is None:
        return None

    def field_text(label, entry):
        name = entry.get("field", "(missing field)")
        field = (fields or {}).get(name, {}) if isinstance(name, str) else {}
        return with_controls(f"{label}: {name}", field)

    kind, value = hit
    level = data["levels"][level_idx]

    if kind in (HIT_FLOOR, HIT_INACCESSIBLE_FLOOR, HIT_LIGHT_BRIDGE):
        list_name = {
            HIT_FLOOR: "floors",
            HIT_INACCESSIBLE_FLOOR: "inaccessible_floors",
            HIT_LIGHT_BRIDGE: "light_bridges",
        }[kind]
        entry = next(e for e in level[list_name] if (e["col"], e["row"]) == value)
        if kind == HIT_LIGHT_BRIDGE:
            return field_text("Light bridge", entry)
        return f"{kind}\n{materials_summary(entry)}"

    if kind == HIT_TERRAIN:
        entry = next(e for e in level["terrain"] if (e["col"], e["row"]) == value)
        return f"Terrain\n{materials_summary(entry, terrain=True)}"

    if kind in (HIT_WALL, HIT_BARRIER):
        list_name = "walls" if kind == HIT_WALL else "barriers"
        entry = next(e for e in level[list_name] if edge_key(e) == value)
        if kind == HIT_BARRIER:
            return field_text("Barrier", entry)
        return f"Wall\n{materials_summary(entry)}"

    if kind == HIT_LIGHT:
        entry = next(light for light in level["lights"] if (light["col"], light["row"], light["side"]) == value)
        return f"Light: {entry.get('kind', '(missing style)')}\n{SIDE_LABELS.get(value[2], value[2])} wall"

    if kind == HIT_LADDER:
        ladder = next(e for e in data["ladders"] if ladder_key(e) == value)
        side = SIDE_LABELS.get(ladder["side"], ladder["side"])
        lower = ladder["lower_level"]
        return f"Ladder: {side}\nLevels {lower} → {lower + ladder['levels']}"

    if kind == HIT_PRESSURE_PLATE:
        return "\n".join(
            pressure_plate_label(plate)
            for plate in data["pressure_plates"]
            if plate["level"] == level_idx and (plate["col"], plate["row"]) == value
        )

    if kind == HIT_ITEM:
        item = next(e for e in data[ITEMS_LIST] if e["level"] == level_idx and (e["col"], e["row"]) == value)
        label = item["type"].replace("_", " ").capitalize()
        return f"{label}: {item['field']}" if "field" in item else label

    if kind in (HIT_SPAWN_ZONE, HIT_CHECKPOINT):
        list_name, index = value
        zone = data[list_name][index]
        if list_name == ACTOR_ZONE_LIST:
            label = f"Actor spawn zone: {zone['kind']}\n{actor_count_preview(zone['count'])}"
            if zone.get("switch"):
                label += f"\nSwitch: {zone['switch']}"
            if zone.get("until_checkpoint") is not None:
                label += f"\nUntil checkpoint {zone['until_checkpoint']}"
                if zone.get("on_checkpoint") == "destroy":
                    label += " (self-destruct)"
            return label
        if is_start(zone):
            return "Start"
        return f"Checkpoint {zone.get('number', '?')}: {CHECKPOINT_TYPE_LABELS.get(zone['type'], zone['type'])}"

    if kind == HIT_RAMP:
        ramp = next(e for e in data["ramps"] if ramp_key(e) == value)
        shape = "Plank\n" if ramp["shape"] == "plank" else ""
        return f"Ramp\nLevels {value[0]} → {value[0] + ramp['levels']}\n{shape}{materials_summary(ramp)}"

    if kind == HIT_NESTED_MAP:
        entry = next(e for e in data[NESTED_MAPS_LIST] if nested_map_key(e) == value)
        label = f"Nested map: {entry['map']}\nLevel {entry['level']}"
        if (entry["level"], entry["from"], entry["from_nudge"]) != (entry["to_level"], entry["to"], entry["to_nudge"]):
            motion = entry["motion"]
            label += f" → Level {entry['to_level']}"
            label += f"\n{MOTION_LABELS.get(motion, motion)} · Travel: {entry['travel_secs']:g} s"
            if motion_uses_cycle(motion):
                label += f" · Pause: {entry['pause_secs']:g} s"
                if entry["phase_secs"]:
                    label += f"\nPhase: {entry['phase_secs']:g} s"
        return with_controls(label, entry)

    return kind

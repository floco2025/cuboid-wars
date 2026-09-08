"""Descriptions of the map element picked under the cursor."""

from __future__ import annotations

from .constants import (
    ACTOR_ZONE_LIST,
    ITEMS_LIST,
    MODE_FLOOR,
    MODE_INACCESSIBLE_FLOOR,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    NESTED_MAPS_LIST,
)
from .display import materials_summary, pressure_plate_label
from .normalization import edge_key, ladder_key, nested_map_key

SIDE_LABELS = {"N": "North", "S": "South", "E": "East", "W": "West"}


def element_hover_text(data: dict, level_idx: int, hit) -> str | None:
    if hit is None:
        return None
    kind, value = hit
    level = data["levels"][level_idx]

    if kind in (MODE_FLOOR, MODE_INACCESSIBLE_FLOOR, MODE_LIGHT_BRIDGE):
        list_name = {
            MODE_FLOOR: "floors",
            MODE_INACCESSIBLE_FLOOR: "inaccessible_floors",
            MODE_LIGHT_BRIDGE: "light_bridges",
        }[kind]
        entry = next(e for e in level[list_name] if (e["col"], e["row"]) == value)
        if kind == MODE_LIGHT_BRIDGE:
            return f"Light bridge: {entry['kind']}"
        return f"{kind}\n{materials_summary(entry)}"

    if kind in ("Wall", "Barrier"):
        list_name = "walls" if kind == "Wall" else "barriers"
        entry = next(e for e in level[list_name] if edge_key(e) == value)
        if kind == "Barrier":
            return f"Barrier: {entry['kind']}"
        return f"Wall\n{materials_summary(entry)}"

    if kind == "Light":
        entry = next(light for light in level["lights"] if (light["col"], light["row"], light["side"]) == value)
        return f"Light: {entry.get('kind', '(missing style)')}\n{SIDE_LABELS.get(value[2], value[2])} wall"

    if kind == "Ladder":
        ladder = next(e for e in data["ladders"] if ladder_key(e) == value)
        side = SIDE_LABELS.get(ladder["side"], ladder["side"])
        lower = ladder["lower_level"]
        return f"Ladder: {side}\nLevels {lower} → {lower + ladder['levels']}"

    if kind == "Pressure Plate":
        return "\n".join(
            pressure_plate_label(plate)
            for plate in data["pressure_plates"]
            if plate["level"] == level_idx and (plate["col"], plate["row"]) == value
        )

    if kind == "Item":
        item = next(
            e for e in data[ITEMS_LIST]
            if e["level"] == level_idx and (e["col"], e["row"]) == value
        )
        label = item["type"].replace("_", " ").capitalize()
        return f"{label}: {item['kind']}" if "kind" in item else label

    if kind == "Spawn Zone":
        list_name, index = value
        zone = data[list_name][index]
        if list_name == ACTOR_ZONE_LIST:
            return f"Actor spawn zone: {zone['kind']}\nCount: {zone['count']}"
        return "Player spawn zone"

    if kind == "Ramp":
        lower = value[0]
        ramp = next(
            e for e in data["ramps"]
            if (e["lower_level"], tuple(e["low"]), tuple(e["high"])) == value
        )
        return f"Ramp\nLevels {lower} → {lower + 1}\n{materials_summary(ramp)}"

    if kind == MODE_NESTED_MAP:
        entry = next(e for e in data[NESTED_MAPS_LIST] if nested_map_key(e) == value)
        label = f"Nested map: {entry['map']}\nLevel {entry['level']}"
        if (entry["level"], entry["from"], entry["from_nudge"]) != (
            entry["to_level"], entry["to"], entry["to_nudge"]
        ):
            label += f" → Level {entry['to_level']}"
            label += f"\nTravel: {entry['travel_secs']:g} s · Pause: {entry['pause_secs']:g} s"
            if entry["phase_secs"]:
                label += f"\nPhase: {entry['phase_secs']:g} s"
        return label

    return kind

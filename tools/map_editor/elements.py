"""Element references shared by picking, selection, and property editing."""

from dataclasses import dataclass

from . import constants as c
from .geometry import rects_overlap, wall_overlaps_rect
from .normalization import edge_key, ladder_key, nested_map_key
from .transforms import EDGE_LISTS, record_lists, record_levels, record_rect

ELEMENT_MODES = {
    "floors": c.MODE_FLOOR,
    "inaccessible_floors": c.MODE_INACCESSIBLE_FLOOR,
    "terrain": c.MODE_TERRAIN,
    "walls": c.MODE_WALL,
    "ramps": c.MODE_RAMP_UP,
    "ladders": c.MODE_LADDER,
    "nested_maps": c.MODE_NESTED_MAP,
    "actor_spawn_zones": c.MODE_ACTOR_SPAWN_ZONE,
    "player_spawn_zones": c.MODE_PLAYER_SPAWN_ZONE,
    "checkpoints": c.MODE_CHECKPOINT,
    "items": c.MODE_ITEM,
    "barriers": c.MODE_BARRIER,
    "erasers": c.MODE_EQUIPMENT_ERASER,
    "light_bridges": c.MODE_LIGHT_BRIDGE,
    "pressure_plates": c.MODE_PRESSURE_PLATE,
    "lights": c.MODE_LIGHT,
}
HIT_LISTS = {
    c.HIT_FLOOR: "floors",
    c.HIT_INACCESSIBLE_FLOOR: "inaccessible_floors",
    c.HIT_TERRAIN: "terrain",
    c.HIT_WALL: "walls",
    c.HIT_RAMP: "ramps",
    c.HIT_LADDER: "ladders",
    c.HIT_NESTED_MAP: "nested_maps",
    c.HIT_ITEM: "items",
    c.HIT_BARRIER: "barriers",
    c.HIT_EQUIPMENT_ERASER: "erasers",
    c.HIT_LIGHT_BRIDGE: "light_bridges",
    c.HIT_PRESSURE_PLATE: "pressure_plates",
    c.HIT_LIGHT: "lights",
}


@dataclass(frozen=True)
class ElementRef:
    name: str
    index: int
    level: int | None = None

    def entries(self, data):
        return (data if self.level is None else data["levels"][self.level]).get(self.name, [])

    def get(self, data):
        return self.entries(data)[self.index]


def element_refs(data):
    for (level, name), entries in record_lists(data):
        for index, entry in enumerate(entries):
            yield ElementRef(name, index, level), entry


def refs_for_hit(data, level, hit):
    if hit is None:
        return []
    kind, value = hit
    if kind in (c.HIT_SPAWN_ZONE, c.HIT_CHECKPOINT):
        return [ElementRef(value[0], value[1])]
    name = HIT_LISTS[kind]
    result = []
    for ref, entry in element_refs(data):
        if ref.name != name or (ref.level is not None and ref.level != level):
            continue
        if name in EDGE_LISTS:
            matches = edge_key(entry) == value
        elif name == "ramps":
            matches = (entry["lower_level"], tuple(entry["low"]), tuple(entry["high"])) == value
        elif name == "ladders":
            matches = ladder_key(entry) == value
        elif name == "nested_maps":
            matches = nested_map_key(entry) == value
        elif name == "lights":
            matches = (entry["col"], entry["row"], entry["side"]) == value
        else:
            matches = (entry["col"], entry["row"]) == value and entry.get("level", level) == level
        if matches:
            result.append(ref)
    return result


def refs_in_region(data, region, excluded=()):
    selected = []
    for ref, entry in element_refs(data):
        if ref.name in excluded:
            continue
        low, high = record_levels(entry, ref.level)
        if low >= region.top or high < region.level:
            continue
        if ref.name in EDGE_LISTS:
            touches = wall_overlaps_rect([entry[k] for k in ("c0", "r0", "c1", "r1")], region.rect)
        else:
            touches = rects_overlap(record_rect(ref.name, entry), region.rect)
        if touches:
            selected.append(ref)
    return selected


def filtered_map(data, excluded):
    result = dict(data)
    result["levels"] = [dict(level) for level in data["levels"]]
    for (level, name), _ in record_lists(data):
        if name in excluded:
            (result if level is None else result["levels"][level])[name] = []
    return result


def restore_excluded(before, after, excluded):
    # Structural edits can move protected records between levels; refusing
    # those is safer than treating a new level index as the same layer.
    if excluded and len(before["levels"]) != len(after["levels"]):
        raise ValueError("Show and unlock all element types before changing the level count.")
    result = dict(after)
    result["levels"] = [dict(level) for level in after["levels"]]
    for (level, name), entries in record_lists(before):
        if name in excluded:
            (result if level is None else result["levels"][level])[name] = entries
    return result

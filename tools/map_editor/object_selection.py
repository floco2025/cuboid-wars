"""Object clipboard blocks preserve the unselected contents around them."""

import copy

from .constants import ITEMS_LIST, LIGHT_SIDES
from .elements import element_refs
from .geometry import wall_endpoints_for_cell_side
from .normalization import edge_key, empty_level, empty_map, ladders_overlap, light_key, normalize_map
from .regions import TileRegion
from .transforms import GLOBAL_LISTS, record_levels, record_lists, record_rect, translate_entry, translate_map


FLOOR_LISTS = ("floors", "inaccessible_floors", "terrain")


# Items and plates stand on floors and lights hang on walls, so removing or
# moving the support takes what it held, as the erase tools do. A cell that
# keeps another floor record still supports its contents.
def with_supported(data, refs):
    chosen = set(refs)
    taken, kept = set(), set()
    for ref, entry in element_refs(data):
        if ref.name in FLOOR_LISTS:
            (taken if ref in chosen else kept).add((ref.level, entry["col"], entry["row"]))
        elif ref.name == "walls":
            (taken if ref in chosen else kept).add((ref.level, edge_key(entry)))
    lost = taken - kept
    result = list(refs)
    for ref, entry in element_refs(data):
        if ref in chosen:
            continue
        if ref.name in (ITEMS_LIST, "pressure_plates"):
            held = (entry["level"], entry["col"], entry["row"]) in lost
        elif ref.name == "lights" and entry["side"] in LIGHT_SIDES:
            held = (ref.level, wall_endpoints_for_cell_side(entry["col"], entry["row"], entry["side"])) in lost
        else:
            held = False
        if held:
            result.append(ref)
    return result


def selected_data(data, refs, *, remove=False):
    chosen = set(refs)
    result = copy.deepcopy(data)
    for (level, name), entries in record_lists(result):
        indices = {ref.index for ref in chosen if ref.level == level and ref.name == name}
        entries[:] = [entry for index, entry in enumerate(entries) if (index in indices) != remove]
    return result


# The region spans the selected records and, for a nested map, its child's
# footprint, which a transform needs inside the block; it may overhang the
# grid, since a placed footprint may.
def object_region(data, refs, definitions):
    rectangles, levels = [], []
    for ref in refs:
        entry = ref.get(data)
        rect = record_rect(ref.name, entry)
        if ref.name == "nested_maps" and entry["map"] in definitions:
            child = definitions[entry["map"]]
            rect = (rect[0], rect[1], rect[2] - 1 + child["grid_cols"], rect[3] - 1 + child["grid_rows"])
        rectangles.append(rect)
        levels.append(record_levels(entry, ref.level))
    c0, r0 = min(rect[0] for rect in rectangles), min(rect[1] for rect in rectangles)
    c1, r1 = max(rect[2] for rect in rectangles), max(rect[3] for rect in rectangles)
    if c0 == c1:
        c0 = min(c0, data["grid_cols"] - 1)
        c1 = c0 + 1
    if r0 == r1:
        r0 = min(r0, data["grid_rows"] - 1)
        r1 = r0 + 1
    lower, upper = min(span[0] for span in levels), max(span[1] for span in levels)
    return TileRegion((c0, r0, c1, r1), lower, upper - lower + 1)


def copy_objects(data, refs, definitions):
    region = object_region(data, refs, definitions)
    c0, r0, c1, r1 = region.rect
    block = {**empty_map(c1 - c0, r1 - r0), **{name: [] for name in GLOBAL_LISTS}}
    block["levels"] = [empty_level(index) for index in range(region.levels)]
    chosen = set(refs)
    for ref, entry in element_refs(data):
        if ref in chosen:
            target = block if ref.level is None else block["levels"][ref.level - region.level]
            target[ref.name].append(translate_entry(ref.name, entry, -c0, -r0, -region.level))
    return block, region


# Records are judged one by one against the grid, not the block's rectangle:
# a nested map's footprint may overhang while its anchor cells fit.
def _within_grid(name, entry, data):
    c0, r0, c1, r1 = record_rect(name, entry)
    return 0 <= c0 <= c1 <= data["grid_cols"] and 0 <= r0 <= r1 <= data["grid_rows"]


def block_fits(block, cell, data):
    moved = translate_map(block, *cell)
    return all(_within_grid(name, entry, data) for (_, name), entries in record_lists(moved) for entry in entries)


def _records_collide(name, a, b, level):
    if name == "lights":
        return light_key(a) == light_key(b)
    if name == "ladders":
        return ladders_overlap(a, b)
    return record_rect(name, a) == record_rect(name, b) and record_levels(a, level) == record_levels(b, level)


def paste_objects(data, block, cell, level):
    if level < 0:
        raise ValueError("The selected levels are outside the map.")
    after = copy.deepcopy(data)
    while len(after["levels"]) < level + len(block["levels"]):
        after["levels"].append(empty_level(len(after["levels"])))
    moved = translate_map(block, *cell, level)
    for (offset, name), entries in record_lists(moved):
        target = after if offset is None else after["levels"][level + offset]
        existing = target.setdefault(name, [])
        for entry in entries:
            if not _within_grid(name, entry, after):
                raise ValueError("The block does not fit inside the map. Choose another tile or resize the map.")
            if any(_records_collide(name, other, entry, offset) for other in existing):
                raise ValueError(f"The destination already contains {name.replace('_', ' ')} here.")
        existing.extend(entries)
    return after


def refs_for_block(data, block, cell, level):
    moved = normalize_map(translate_map(block, *cell, level))
    expected = [
        (ref.name, None if ref.level is None else ref.level + level, entry) for ref, entry in element_refs(moved)
    ]
    return [ref for ref, entry in element_refs(data) if (ref.name, ref.level, entry) in expected]

"""Object clipboard blocks preserve the unselected contents around them."""

import copy

from .elements import element_refs
from .normalization import empty_level, normalize_map
from .regions import TileRegion, copy_region
from .transforms import record_levels, record_lists, record_rect, translate_map


def selected_data(data, refs, *, remove=False):
    chosen = set(refs)
    result = copy.deepcopy(data)
    for (level, name), entries in record_lists(result):
        indices = {ref.index for ref in chosen if ref.level == level and ref.name == name}
        entries[:] = [entry for index, entry in enumerate(entries) if (index in indices) != remove]
    return result


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
    return copy_region(selected_data(data, refs), region), region


def paste_objects(data, block, cell, level):
    col, row = cell
    destination = TileRegion(
        (col, row, col + block["grid_cols"], row + block["grid_rows"]), level, len(block["levels"])
    )
    after = copy.deepcopy(data)
    while len(after["levels"]) < destination.top:
        after["levels"].append(empty_level(len(after["levels"])))
    destination.check_bounds(after)
    moved = translate_map(block, col, row, level)
    for (offset, name), entries in record_lists(moved):
        target = after if offset is None else after["levels"][level + offset]
        existing = target.setdefault(name, [])
        for entry in entries:
            if any(
                record_rect(name, other) == record_rect(name, entry)
                and record_levels(other, offset) == record_levels(entry, offset)
                for other in existing
            ):
                raise ValueError(f"The destination already contains {name.replace('_', ' ')} here.")
        existing.extend(entries)
    return after


def refs_for_block(data, block, cell, level):
    moved = normalize_map(translate_map(block, *cell, level))
    expected = [
        (ref.name, None if ref.level is None else ref.level + level, entry) for ref, entry in element_refs(moved)
    ]
    return [ref for ref, entry in element_refs(data) if (ref.name, ref.level, entry) in expected]

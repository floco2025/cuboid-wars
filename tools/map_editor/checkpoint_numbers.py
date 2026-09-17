"""Checkpoint numbers: one course sequence per map document."""

import copy

from .constants import ACTOR_ZONE_LIST, CHECKPOINT_LIST
from .validation import placed_definitions


def geometries(root: dict):
    yield None, root
    yield from root.get("nested_geometry", {}).items()


def _is_number(value) -> bool:
    return type(value) is int and value >= 1


def _numbers(geometry: dict) -> set[int]:
    return {entry["number"] for entry in geometry.get(CHECKPOINT_LIST, []) if _is_number(entry.get("number"))}


# Renumbers the geometry's checkpoints in place; its zones ending at one follow.
def _apply_mapping(geometry: dict, mapping: dict[int, int]) -> None:
    for entry in geometry.get(CHECKPOINT_LIST, []):
        if entry.get("number") in mapping:
            entry["number"] = mapping[entry["number"]]
    for zone in geometry.get(ACTOR_ZONE_LIST, []):
        if zone.get("until_checkpoint") in mapping:
            zone["until_checkpoint"] = mapping[zone["until_checkpoint"]]


def used_numbers(root: dict) -> set[int]:
    return set().union(*(_numbers(geometry) for _, geometry in geometries(root)))


def next_checkpoint_number(root: dict) -> int:
    return max(used_numbers(root), default=0) + 1


def checkpoint_entries(root: dict) -> list[tuple[str | None, dict]]:
    """Every checkpoint of the document with its definition's name (`None`
    for the outer map), in course order; unnumbered ones last."""
    entries = [(name, entry) for name, geometry in geometries(root) for entry in geometry.get(CHECKPOINT_LIST, [])]
    entries.sort(key=lambda item: (not _is_number(item[1].get("number")), item[1].get("number") or 0, item[0] or ""))
    return entries


def number_checkpoint_copies(block: dict, root: dict) -> dict:
    """The block with fresh numbers for the checkpoints whose numbers the
    document already uses; the block's zones ending at them follow."""
    block = copy.deepcopy(block)
    used = used_numbers(root)
    next_free = max(used | _numbers(block), default=0) + 1
    mapping = {}
    for number in sorted(_numbers(block) & used):
        mapping[number] = next_free
        next_free += 1
    _apply_mapping(block, mapping)
    return block


def number_generated_definitions(root: dict, generated: set[str]) -> dict:
    """The document with the checkpoints of the generated definitions
    renumbered where they collide with the rest of the placed map tree, each
    definition's own zones following. A copy whose original is no longer
    placed keeps its numbers, so the zones ending at them keep pointing there."""
    after = copy.deepcopy(root)
    placed = placed_definitions(after, after.get("nested_geometry", {}))
    taken = set().union(_numbers(after), *(_numbers(g) for name, g in placed.items() if name not in generated))
    next_free = next_checkpoint_number(after)
    for name in sorted(generated):
        geometry = placed.get(name)
        if geometry is None:
            continue
        mapping = {}
        for number in sorted(_numbers(geometry) & taken):
            mapping[number] = next_free
            next_free += 1
        _apply_mapping(geometry, mapping)
        taken |= _numbers(geometry)
    return after


def renumber_checkpoints(root: dict, mapping: dict[int, int]) -> dict:
    """The document with the checkpoints in `mapping` renumbered at once,
    every zone ending at one following it."""
    targets = list(mapping.values())
    if len(set(targets)) != len(targets) or not all(_is_number(number) for number in targets):
        raise ValueError("Checkpoint numbers must be distinct positive whole numbers.")
    after = copy.deepcopy(root)
    for _, geometry in geometries(after):
        _apply_mapping(geometry, mapping)
    numbers = [entry.get("number") for _, entry in checkpoint_entries(after) if _is_number(entry.get("number"))]
    if len(set(numbers)) != len(numbers):
        raise ValueError("Checkpoint numbers must be unique in the map.")
    return after

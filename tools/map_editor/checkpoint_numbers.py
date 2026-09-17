"""Checkpoint numbers: one course sequence per map document."""

import copy

from .constants import ACTOR_ZONE_LIST, CHECKPOINT_LIST


def geometries(root: dict):
    yield None, root
    yield from root.get("nested_geometry", {}).items()


def _is_number(value) -> bool:
    return type(value) is int and value >= 1


def used_numbers(root: dict) -> set[int]:
    return {
        entry["number"]
        for _, geometry in geometries(root)
        for entry in geometry.get(CHECKPOINT_LIST, [])
        if _is_number(entry.get("number"))
    }


def next_checkpoint_number(root: dict) -> int:
    return max(used_numbers(root), default=0) + 1


def checkpoint_entries(root: dict) -> list[tuple[str | None, dict]]:
    """Every checkpoint of the document with its definition's name (`None`
    for the outer map), in course order; unnumbered ones last."""
    entries = [(name, entry) for name, geometry in geometries(root) for entry in geometry.get(CHECKPOINT_LIST, [])]
    entries.sort(key=lambda item: (not _is_number(item[1].get("number")), item[1].get("number") or 0, item[0] or ""))
    return entries


def number_checkpoint_copies(block: dict, root: dict) -> dict:
    """The block with fresh numbers for the checkpoints whose numbers the document already uses."""
    block = copy.deepcopy(block)
    used = used_numbers(root)
    reserved = used | {entry["number"] for entry in block.get(CHECKPOINT_LIST, []) if _is_number(entry.get("number"))}
    next_free = max(reserved, default=0) + 1
    for entry in block.get(CHECKPOINT_LIST, []):
        number = entry.get("number")
        if not _is_number(number):
            continue
        if number in used:
            entry["number"] = next_free
            next_free += 1
        used.add(entry["number"])
    return block


def renumber_checkpoints(root: dict, mapping: dict[int, int]) -> dict:
    """The document with the checkpoints in `mapping` renumbered at once,
    every zone ending at one following it."""
    targets = list(mapping.values())
    if len(set(targets)) != len(targets) or not all(_is_number(number) for number in targets):
        raise ValueError("Checkpoint numbers must be distinct positive whole numbers.")
    after = copy.deepcopy(root)
    for _, geometry in geometries(after):
        for entry in geometry.get(CHECKPOINT_LIST, []):
            if entry.get("number") in mapping:
                entry["number"] = mapping[entry["number"]]
        for zone in geometry.get(ACTOR_ZONE_LIST, []):
            if zone.get("until_checkpoint") in mapping:
                zone["until_checkpoint"] = mapping[zone["until_checkpoint"]]
    numbers = [entry.get("number") for _, entry in checkpoint_entries(after) if _is_number(entry.get("number"))]
    if len(set(numbers)) != len(numbers):
        raise ValueError("Checkpoint numbers must be unique in the map.")
    return after

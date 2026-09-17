"""Checkpoint numbers: one course sequence per map document."""

import copy

from .constants import ACTOR_ZONE_LIST, CHECKPOINT_LIST, START_CHECKPOINT


def geometries(root: dict):
    yield None, root
    yield from root.get("nested_geometry", {}).items()


def _is_number(value) -> bool:
    return type(value) is int and value >= START_CHECKPOINT


def is_start(zone: dict) -> bool:
    return _is_number(zone.get("number")) and zone["number"] == START_CHECKPOINT


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
    return max(used_numbers(root), default=START_CHECKPOINT) + 1


def checkpoint_entries(root: dict) -> list[tuple[str | None, dict]]:
    """Every checkpoint of the document with its definition's name (`None`
    for the outer map), in course order; unnumbered ones last."""
    entries = [(name, entry) for name, geometry in geometries(root) for entry in geometry.get(CHECKPOINT_LIST, [])]
    entries.sort(
        key=lambda item: (
            not _is_number(item[1].get("number")),
            item[1]["number"] if _is_number(item[1].get("number")) else 0,
            item[0] or "",
        )
    )
    return entries


def checkpoint_groups(root: dict) -> list[dict]:
    """One entry per distinct number of the document, in course order: the
    number, how many checkpoints carry it, the definitions holding them
    (`None` for the outer map), and the types they use. A checkpoint without
    a valid number is an entry of its own, after the numbered ones."""
    groups: dict[int, dict] = {}
    unnumbered = []
    for name, entry in checkpoint_entries(root):
        number = entry.get("number")
        if not _is_number(number):
            unnumbered.append({"number": None, "instances": 1, "maps": [name], "types": [entry.get("type")]})
            continue
        group = groups.setdefault(number, {"number": number, "instances": 0, "maps": [], "types": []})
        group["instances"] += 1
        if name not in group["maps"]:
            group["maps"].append(name)
        if entry.get("type") not in group["types"]:
            group["types"].append(entry.get("type"))
    return [*groups.values(), *unnumbered]


def renumber_checkpoints(root: dict, mapping: dict[int, int]) -> dict:
    """The document with every checkpoint numbered a key of `mapping` given
    its value, in the outer map and every definition alike, the zones ending
    at one following it."""
    targets = list(mapping.values())
    if START_CHECKPOINT in mapping or START_CHECKPOINT in targets:
        raise ValueError(f"Checkpoint {START_CHECKPOINT} is the start and keeps its number.")
    if len(set(targets)) != len(targets) or not all(_is_number(number) for number in targets):
        raise ValueError("Checkpoint numbers must be distinct positive whole numbers.")
    after = copy.deepcopy(root)
    for _, geometry in geometries(after):
        _apply_mapping(geometry, mapping)
    return after

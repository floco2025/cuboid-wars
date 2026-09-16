"""Checkpoint names for blocks copied into a map definition."""

import copy


def name_checkpoint_copies(block, destination):
    block = copy.deepcopy(block)
    used = {entry["name"] for entry in destination.get("checkpoints", []) if isinstance(entry.get("name"), str)}
    reserved = used | {entry["name"] for entry in block.get("checkpoints", []) if isinstance(entry.get("name"), str)}
    for entry in block.get("checkpoints", []):
        name = entry.get("name")
        if not isinstance(name, str) or not name:
            continue
        if name in used:
            suffix = 2
            while f"{name} {suffix}" in reserved:
                suffix += 1
            entry["name"] = f"{name} {suffix}"
        used.add(entry["name"])
        reserved.add(entry["name"])
    return block

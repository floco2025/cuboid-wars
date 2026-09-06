"""Explicit repair proposals and maintenance scoped to a user's edit."""

from __future__ import annotations

from collections import Counter
import json

from .normalization import canonicalize_map, normalize_map
from .transforms import record_lists


def _key(entry: dict) -> str:
    return json.dumps(entry, sort_keys=True)


def repair_summary(data: dict, repaired: dict) -> list[str]:
    before = dict(record_lists(data))
    after = dict(record_lists(repaired))
    lines = []
    for (level, name), entries in before.items():
        original = Counter(map(_key, entries))
        replacement = Counter(map(_key, after.get((level, name), [])))
        removed, added = sum((original - replacement).values()), sum((replacement - original).values())
        if removed or added:
            prefix = f"Level {level}: " if level is not None else ""
            lines.append(f"{prefix}{name.replace('_', ' ')}: remove/change {removed}, add/change {added}")
    return lines


def maintain_edit(before: dict, after: dict) -> dict:
    normalized = normalize_map(after)
    # Geometry transforms can move invalid records; only explicit repair may remove them.
    if repair_summary(before, canonicalize_map(before)):
        return normalized
    return canonicalize_map(normalized)

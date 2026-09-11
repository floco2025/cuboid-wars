"""Explicit repair proposals."""

from __future__ import annotations

from collections import Counter
import json

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
    if data.get("fireworks") != repaired.get("fireworks"):
        lines.append("fireworks: remove switch_inverted")
    return lines

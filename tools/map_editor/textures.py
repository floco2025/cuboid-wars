"""Per-host texture catalogs from gameplay settings."""

from __future__ import annotations

import json

from .constants import GAMEPLAY_PATH, MAPS_DIR


def load_texture_catalog(host: str) -> dict[str, bool]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        settings = json.load(handle)["maps"][host]
    path = f"maps.{host}.textures"
    value = settings.get("textures")
    if not isinstance(value, dict):
        raise ValueError(f"{path} must be an object of aliases with a portalable boolean")
    result = {}
    for alias, entry in value.items():
        if not alias.strip() or not isinstance(entry, dict) or type(entry.get("portalable")) is not bool:
            raise ValueError(f"{path}.{alias}: expected a nonempty alias and an explicit portalable boolean")
        result[alias] = entry["portalable"]
    return dict(sorted(result.items()))


def texture_hosts(map_name: str | None) -> tuple[str, list[str]]:
    with GAMEPLAY_PATH.open(encoding="utf-8") as handle:
        gameplay = json.load(handle)
    names = sorted(gameplay["maps"])
    if map_name in names:
        return map_name, names

    def contains(name, seen):
        if name == map_name:
            return True
        if name in seen:
            return False
        seen.add(name)
        try:
            with (MAPS_DIR / f"{name}.json").open(encoding="utf-8") as handle:
                nested = json.load(handle)["map"].get("nested_maps", [])
            return any(contains(entry["map"], seen) for entry in nested)
        except (OSError, ValueError, KeyError):
            return False

    preferred = next((name for name in names if contains(name, set())), gameplay["default_map"])
    return preferred, names


def portal_label(portalable: bool) -> str:
    return "Portals allowed" if portalable else "Portals incompatible"

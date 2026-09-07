"""Per-host texture catalogs from gameplay settings."""

from __future__ import annotations

import json

from .constants import GAMEPLAY_PATH


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


def portal_label(portalable: bool) -> str:
    return "Portals allowed" if portalable else "Portals incompatible"

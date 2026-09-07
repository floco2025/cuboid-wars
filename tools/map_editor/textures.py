"""Per-host texture catalogs from gameplay settings."""

from __future__ import annotations

from .constants import load_map_settings, map_settings_path


def load_texture_catalog(host: str) -> dict[str, bool]:
    settings = load_map_settings(host)
    path = f"{map_settings_path(host)}: textures"
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

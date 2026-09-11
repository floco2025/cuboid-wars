"""Control identities and their references throughout a map document."""

import copy

from .catalogs import HEX_COLOR, SWITCH_ACTIVATIONS, SWITCH_HOLDS, SWITCH_RESETS


def geometries(root: dict):
    yield root
    yield from root.get("nested_geometry", {}).values()


def references(root: dict, catalog: str):
    for geometry in geometries(root):
        if catalog == "switch_kinds":
            for name in ("pressure_plates", "actor_spawn_zones", "nested_maps"):
                for entry in geometry.get(name, []):
                    yield entry, "switch"
        if catalog == "barrier_kinds":
            for item in geometry.get("items", []):
                if item.get("type") == "key":
                    yield item, "kind"
        for level in geometry["levels"]:
            for name in ("barriers", "light_bridges"):
                for entry in level.get(name, []):
                    if catalog == "switch_kinds":
                        yield entry, "switch"
                    elif (catalog, name) in (("barrier_kinds", "barriers"), ("bridge_kinds", "light_bridges")):
                        yield entry, "kind"
    if catalog == "switch_kinds" and root.get("fireworks"):
        yield root["fireworks"], "switch"


def validate_catalog(catalog: str, entries: list[dict]) -> None:
    if not isinstance(entries, list) or any(not isinstance(entry, dict) for entry in entries):
        raise ValueError(f"{catalog}: expected a list of kind definitions")
    if catalog == "barrier_kinds" and len(entries) > 256:
        raise ValueError("barrier_kinds: at most 256 kinds fit in the key inventory")
    seen = set()
    for entry in entries:
        name = entry.get("id")
        if not isinstance(name, str) or not name.strip() or name != name.strip() or name in seen:
            raise ValueError(f"{catalog}: names must be nonempty, unique, and have no surrounding spaces")
        seen.add(name)
        color = entry.get("plate_color") if catalog == "switch_kinds" else entry.get("color")
        if color is not None and (not isinstance(color, str) or not HEX_COLOR.fullmatch(color)):
            raise ValueError(f"{name}: color must look like #rrggbb")
        if catalog != "switch_kinds" and color is None:
            raise ValueError(f"{name}: a color is required")
        if catalog == "switch_kinds":
            for field, choices in (
                ("activation", SWITCH_ACTIVATIONS),
                ("reset_on_player_death", SWITCH_RESETS),
                ("held", SWITCH_HOLDS),
            ):
                if entry.get(field, "any" if field == "held" else None) not in choices:
                    raise ValueError(f"{name}: {field} must be one of {', '.join(choices)}")


def edit_catalog(root: dict, catalog: str, entries: list[dict], renames: dict[str, str]) -> dict:
    validate_catalog(catalog, entries)
    after = copy.deepcopy(root)
    source = after.setdefault("_settings", {}) if catalog in ("barrier_kinds", "bridge_kinds") else after
    previous = {entry["id"] for entry in source.get(catalog, [])}
    kept = {entry["id"] for entry in entries} & previous - set(renames.values())
    removed = previous - set(renames) - kept
    for entry, field in references(after, catalog):
        value = entry.get(field)
        if value in removed:
            raise ValueError(f"{value!r} is still assigned to map objects; reassign them before deleting it")
        if value in renames:
            entry[field] = renames[value]
    source[catalog] = copy.deepcopy(entries)
    return after

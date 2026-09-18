"""Control identities and their references throughout a map document."""

import copy

from .core import call


def geometries(root: dict):
    yield root
    yield from root.get("nested_geometry", {}).values()


def references(root: dict, catalog: str):
    for geometry in geometries(root):
        if catalog == "switches":
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
                    if catalog == "switches":
                        yield entry, "switch"
                    elif (catalog, name) in (("barrier_kinds", "barriers"), ("bridge_kinds", "light_bridges")):
                        yield entry, "kind"
    if catalog == "switches" and root.get("fireworks"):
        yield root["fireworks"], "switch"


def validate_catalog(catalog: str, entries: list[dict]) -> None:
    call("validate_catalog", catalog, entries)


def edit_catalog(root: dict, catalog: str, entries: list[dict], renames: dict[str, str]) -> dict:
    validate_catalog(catalog, entries)
    after = copy.deepcopy(root)
    previous = {entry["id"] for entry in after.get(catalog, [])}
    kept = {entry["id"] for entry in entries} & previous - set(renames.values())
    removed = previous - set(renames) - kept
    for entry, field in references(after, catalog):
        value = entry.get(field)
        if value in removed:
            raise ValueError(f"{value!r} is still assigned to map objects; reassign them before deleting it")
        if value in renames:
            entry[field] = renames[value]
    after[catalog] = copy.deepcopy(entries)
    return after

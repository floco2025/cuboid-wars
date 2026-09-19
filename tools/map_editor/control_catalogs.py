"""Control identities and their references throughout a map document."""

import copy

from .catalogs import field_entries
from .core import call


def geometries(root: dict):
    yield root
    yield from root.get("nested_geometry", {}).values()


# Every record that may name an entry of `catalog`, with the list it sits in
# and the key that holds the name. Fields are the root's alone.
def named_references(root: dict, catalog: str):
    if catalog == "switches":
        for entry in field_entries(root):
            yield "fields", entry, "switch"
        if root.get("fireworks"):
            yield "fireworks", root["fireworks"], "switch"
    for geometry in geometries(root):
        if catalog == "switches":
            for name in ("pressure_plates", "actor_spawn_zones", "nested_maps"):
                for entry in geometry.get(name, []):
                    yield name, entry, "switch"
        else:
            for item in geometry.get("items", []):
                if item.get("type") == "key":
                    yield "items", item, "field"
            for level in geometry["levels"]:
                for name in ("barriers", "light_bridges"):
                    for entry in level.get(name, []):
                        yield name, entry, "field"


def references(root: dict, catalog: str):
    for _name, entry, key in named_references(root, catalog):
        yield entry, key


USAGE_NOUNS = {
    "pressure_plates": ("plate", "plates"),
    "fields": ("field", "fields"),
    "barriers": ("barrier", "barriers"),
    "light_bridges": ("bridge cell", "bridge cells"),
    "actor_spawn_zones": ("actor zone", "actor zones"),
    "nested_maps": ("nested map", "nested maps"),
    "items": ("key", "keys"),
    "fireworks": ("fireworks", "fireworks"),
}


# What names each entry of `catalog` across the outer map and every nested
# definition, as text like "2 plates, 3 barriers"; an unused entry is absent.
def catalog_usage(root: dict, catalog: str) -> dict[str, str]:
    counts: dict[str, dict[str, int]] = {}
    for name, entry, key in named_references(root, catalog):
        value = entry.get(key)
        if value is not None:
            uses = counts.setdefault(value, {})
            uses[name] = uses.get(name, 0) + 1
    return {
        value: ", ".join(f"{uses[name]} {USAGE_NOUNS[name][uses[name] != 1]}" for name in USAGE_NOUNS if name in uses)
        for value, uses in counts.items()
    }


def validate_catalog(catalog: str, entries: list[dict]) -> None:
    call("validate_catalog", catalog, entries)


def edit_catalog(root: dict, catalog: str, entries: list[dict], renames: dict[str, str]) -> dict:
    validate_catalog(catalog, entries)
    after = copy.deepcopy(root)
    previous = {entry["id"] for entry in after.get(catalog, [])}
    kept = {entry["id"] for entry in entries} & previous - set(renames.values())
    removed = previous - set(renames) - kept
    for entry, key in references(after, catalog):
        value = entry.get(key)
        if value in removed:
            raise ValueError(f"{value!r} is still assigned to map objects; reassign them before deleting it")
        if value in renames:
            entry[key] = renames[value]
    after[catalog] = copy.deepcopy(entries)
    return after

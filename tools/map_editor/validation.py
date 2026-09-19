"""Python views of the shared map-core diagnostics."""

from __future__ import annotations

import re
from dataclasses import dataclass

from .catalogs import MapCatalogs
from .core import call, shapes

_INDEX_RE = re.compile(r"\[\d+\]")


# A warning names something inert until another record exists (a switch no
# plate operates yet): Check Map lists it, and it blocks no edit and no save.
@dataclass(frozen=True)
class ValidationIssue:
    message: str
    level: int | None = None
    rect: tuple[int, int, int, int] | None = None
    map_name: str | None = None
    warning: bool = False

    # The issue without its list index, which shifts whenever an earlier
    # record comes or goes: what an edit added is judged by this.
    def identity(self) -> tuple:
        return (self.map_name, self.level, self.rect, _INDEX_RE.sub("[]", self.message))


# The messages of the errors; `issues` holds the warnings too.
class ValidationErrors(list):
    def __init__(self, rows=()):
        self.issues = [
            ValidationIssue(**{**row, "rect": tuple(row["rect"]) if row["rect"] is not None else None}) for row in rows
        ]
        super().__init__(issue.message for issue in self.issues if not issue.warning)

    @property
    def errors(self) -> list[ValidationIssue]:
        return [issue for issue in self.issues if not issue.warning]

    @property
    def warnings(self) -> list[str]:
        return [issue.message for issue in self.issues if issue.warning]


def placed_definitions(root: dict, definitions: dict) -> dict[str, dict]:
    return call("placed_definitions", root, definitions)


def document_checkpoint_numbers(geometries: list[dict]) -> set[int]:
    return set(call("document_checkpoint_numbers", geometries))


def plated_switches(geometries: list[dict]) -> set[str]:
    return set(call("plated_switches", geometries))


def validate_map(
    map_data: dict,
    fields: list[str],
    *,
    switches: list[str] | None = None,
    plated_switches: set[str] | None = None,
    map_name: str | None = None,
    nested_lookup=None,
    actor_kinds: list[str] | None = None,
    material_aliases: list[str] | None = None,
    wall_light_kinds: list[str] | None = None,
    checkpoint_numbers: set[int] | None = None,
) -> ValidationErrors:
    context = dict(
        fields=fields,
        switches=switches,
        plated_switches=plated_switches,
        map_name=map_name,
        nested_shapes=shapes(map_data.get("nested_maps", []), nested_lookup) if nested_lookup is not None else None,
        actor_kinds=actor_kinds,
        material_aliases=material_aliases,
        wall_light_kinds=wall_light_kinds,
        checkpoint_numbers=checkpoint_numbers,
    )
    return ValidationErrors(call("validate_map", map_data, context))


def validate_document(
    root: dict,
    catalogs: MapCatalogs,
    *,
    actor_kinds: list[str] | None = None,
    wall_light_kinds: list[str] | None = None,
) -> ValidationErrors:
    return ValidationErrors(
        call(
            "validate_document",
            root,
            dict(
                fields=list(catalogs.field_colors),
                switches=list(catalogs.switches),
                material_aliases=list(catalogs.texture_catalog),
                pickup_types=list(catalogs.pickup_types),
                actor_kinds=actor_kinds,
                wall_light_kinds=wall_light_kinds,
            ),
        )
    )

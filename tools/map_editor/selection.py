"""The selection shared by drawing, Properties, handles, and commands."""

from dataclasses import dataclass, replace

from .elements import ElementRef, element_refs, refs_in_region
from .object_selection import object_region
from .regions import TileRegion


@dataclass(frozen=True)
class Selection:
    objects: tuple[ElementRef, ...] = ()
    area: TileRegion | None = None
    anchor: tuple[int, int] | None = None

    def __post_init__(self):
        if self.objects and self.area is not None:
            raise ValueError("A selection contains objects or a tile area.")

    @property
    def empty(self):
        return not self.objects and self.area is None

    def refs(self, data):
        return refs_in_region(data, self.area) if self.area is not None else list(self.objects)

    def region(self, data, definitions):
        if self.area is not None:
            return self.area
        return object_region(data, self.objects, definitions) if self.objects else None

    def contains(self, point, hits):
        if self.area is not None:
            c0, r0, c1, r1 = self.area.rect
            return c0 <= point.x() < c1 and r0 <= point.y() < r1
        return any(ref in self.objects for ref in hits)

    # Records keep no identity across a level insertion or removal, so the
    # selection ends rather than land on whatever shifted into its place.
    def refreshed(self, before, after):
        anchor = self.anchor
        if anchor is not None and not (0 <= anchor[0] < after["grid_cols"] and 0 <= anchor[1] < after["grid_rows"]):
            anchor = None
        if len(before["levels"]) != len(after["levels"]):
            return Selection(anchor=anchor)
        if self.area is not None:
            c0, r0, c1, r1 = self.area.rect
            c1, r1 = min(c1, after["grid_cols"]), min(r1, after["grid_rows"])
            level = min(self.area.level, len(after["levels"]) - 1)
            if c0 >= c1 or r0 >= r1:
                return Selection()
            return replace(
                self, area=TileRegion((c0, r0, c1, r1), level, min(self.area.levels, len(after["levels"]) - level))
            )
        wanted = {}
        for ref in self.objects:
            wanted.setdefault((ref.name, ref.level), []).append(ref.get(before))
        refs = tuple(ref for ref, entry in element_refs(after) if entry in wanted.get((ref.name, ref.level), ()))
        return Selection(refs, anchor=anchor)

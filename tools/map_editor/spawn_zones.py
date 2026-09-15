"""Zone geometry and access through the shared selection."""

from .constants import ZONE_LISTS, ZONE_PICK_ORDER
from .elements import ElementRef
from .geometry import zone_contains_cell, zone_rect, zone_spans_level
from .types import ZoneRef
from .normalization import zone_key


def resized_zone_rect(zone, handle, origin, current, cols, rows):
    dx, dy = round(current.x() - origin.x()), round(current.y() - origin.y())
    c0, r0, c1, r1 = zone_rect(zone)
    if "n" in handle:
        r0 = max(0, min(r1 - 1, r0 + dy))
    if "s" in handle:
        r1 = max(r0 + 1, min(rows, r1 + dy))
    if "w" in handle:
        c0 = max(0, min(c1 - 1, c0 + dx))
    if "e" in handle:
        c1 = max(c0 + 1, min(cols, c1 + dx))
    return c0, r0, c1, r1


class SpawnZoneEditMixin:
    @property
    def selected_spawn_zone_ref(self):
        refs = self.selection.objects
        if len(refs) == 1 and refs[0].name in ZONE_LISTS:
            return ZoneRef(refs[0].name, refs[0].index)
        return None

    def selected_spawn_zone(self):
        ref = self.selected_spawn_zone_ref
        return self.map_data[ref.list_name][ref.index] if ref else None

    def set_selected_spawn_zone(self, ref):
        self.inspect_refs([ElementRef(ref.list_name, ref.index)] if ref else [])

    def _zone_ref_after_change(self, name, zone):
        key = zone_key(name, zone)
        return next(
            (ZoneRef(name, index) for index, entry in enumerate(self.map_data[name]) if zone_key(name, entry) == key),
            None,
        )

    def spawn_zone_at(self, pos):
        col, row = int(pos.x() // 1), int(pos.y() // 1)
        for name in ZONE_PICK_ORDER:
            for index in range(len(self.map_data[name]) - 1, -1, -1):
                zone = self.map_data[name][index]
                if zone_spans_level(zone, self.current_level) and zone_contains_cell(zone, col, row):
                    return ZoneRef(name, index)
        return None

    def selected_spawn_zone_has_fields(self):
        return self.selected_spawn_zone_ref is not None

    def edit_selected_spawn_zone_fields(self):
        self.refresh_inspection(show=True)

"""Spawn-zone selection and edit-drag actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    PLAYER_ZONE_LIST,
    SPAWN_ZONE_HANDLE_PIXELS,
    SPAWN_ZONE_LISTS,
)
from .geometry import zone_contains_cell, zone_rect
from .normalization import zone_key
from .types import SpawnZoneDrag, ZoneRef


class SpawnZoneEditMixin:
    # === Spawn-zone selection and drags, driven by No Tool ===

    def selected_spawn_zone(self) -> dict | None:
        ref = self.selected_spawn_zone_ref
        if ref is None:
            return None
        if not (0 <= ref.index < len(self.map_data[ref.list_name])):
            return None
        return self.map_data[ref.list_name][ref.index]

    def set_selected_spawn_zone(self, ref: ZoneRef | None) -> None:
        if ref is None:
            self.selected_spawn_zone_ref = None
        elif ref.list_name in SPAWN_ZONE_LISTS and 0 <= ref.index < len(self.map_data[ref.list_name]):
            self.selected_spawn_zone_ref = ref
        else:
            self.selected_spawn_zone_ref = None
        self.canvas.update()

    def _find_zone_index(self, list_name: str, target: dict) -> int | None:
        key = zone_key(list_name, target)
        for idx, zone in enumerate(self.map_data[list_name]):
            if zone_key(list_name, zone) == key:
                return idx
        return None

    def _zone_ref_after_change(self, list_name: str, target: dict) -> ZoneRef | None:
        new_idx = self._find_zone_index(list_name, target)
        return ZoneRef(list_name, new_idx) if new_idx is not None else None

    def spawn_zone_at(self, pos, cell_size: float) -> ZoneRef | None:
        col = int(pos.x() // cell_size)
        row = int(pos.y() // cell_size)
        # Priority order when zones overlap a cell: actor → player. Actor
        # zones carry per-zone configuration (kind / count), so the user is
        # more likely to want them. Within each list, iterate in reverse so
        # the most-recently-painted wins.
        for list_name in (ACTOR_ZONE_LIST, PLAYER_ZONE_LIST):
            for idx in range(len(self.map_data[list_name]) - 1, -1, -1):
                zone = self.map_data[list_name][idx]
                if zone["level"] == self.current_level and zone_contains_cell(zone, col, row):
                    return ZoneRef(list_name, idx)
        return None

    def begin_spawn_zone_drag(self, pos, cell_size: float) -> bool:
        """A press on a spawn zone. A handle of the selected zone starts a
        resize and its body a move; any other zone is only selected, so a
        stray drag moves nothing. Returns whether a zone took the press."""
        zone = self.selected_spawn_zone()
        if zone is not None and zone["level"] == self.current_level:
            handle = self._handle_at_pos(zone, pos, cell_size)
            if handle is not None:
                assert self.selected_spawn_zone_ref is not None
                ref = self.selected_spawn_zone_ref
                self.spawn_zone_drag = SpawnZoneDrag(
                    list_name=ref.list_name,
                    index=ref.index,
                    handle=handle,
                    origin=(pos.x() / cell_size, pos.y() / cell_size),
                    original_zone=copy.deepcopy(zone),
                )
                return True
        ref = self.spawn_zone_at(pos, cell_size)
        self.spawn_zone_drag = None
        if ref is None:
            self.set_selected_spawn_zone(None)
            return False
        if ref != self.selected_spawn_zone_ref:
            self.set_selected_spawn_zone(ref)
            return True
        self.spawn_zone_drag = SpawnZoneDrag(
            list_name=ref.list_name,
            index=ref.index,
            handle="move",
            origin=(pos.x() / cell_size, pos.y() / cell_size),
            original_zone=copy.deepcopy(self.map_data[ref.list_name][ref.index]),
        )
        return True

    def _handle_at_pos(self, zone: dict, pos, cell_size: float) -> str | None:
        handle_names = ["nw", "n", "ne", "e", "se", "s", "sw", "w"]
        centers = self.canvas.spawn_zone_handle_centers(zone, cell_size)
        radius = max(SPAWN_ZONE_HANDLE_PIXELS * 0.75, 6.0)
        for name, (cx, cy) in zip(handle_names, centers):
            if abs(pos.x() - cx) <= radius and abs(pos.y() - cy) <= radius:
                return name
        return None

    def update_spawn_zone_edit_drag(self, pos, cell_size: float) -> None:
        if self.spawn_zone_drag is None:
            return
        self.spawn_zone_drag.current = (pos.x() / cell_size, pos.y() / cell_size)

    def spawn_zone_candidate_rect(self) -> tuple[int, int, int, int] | None:
        drag = self.spawn_zone_drag
        if drag is None or drag.current is None:
            return None
        ox, oy = drag.origin
        cx, cy = drag.current
        dx_cells = round(cx - ox)
        dy_cells = round(cy - oy)
        c0, r0, c1, r1 = zone_rect(drag.original_zone)
        cols_max = self.map_data["grid_cols"]
        rows_max = self.map_data["grid_rows"]
        if drag.handle == "move":
            new_c0 = max(0, min(cols_max - (c1 - c0), c0 + dx_cells))
            new_r0 = max(0, min(rows_max - (r1 - r0), r0 + dy_cells))
            return (new_c0, new_r0, new_c0 + (c1 - c0), new_r0 + (r1 - r0))
        new_c0, new_r0, new_c1, new_r1 = c0, r0, c1, r1
        if "n" in drag.handle:
            new_r0 = max(0, min(r1 - 1, r0 + dy_cells))
        if "s" in drag.handle:
            new_r1 = max(r0 + 1, min(rows_max, r1 + dy_cells))
        if "w" in drag.handle:
            new_c0 = max(0, min(c1 - 1, c0 + dx_cells))
        if "e" in drag.handle:
            new_c1 = max(c0 + 1, min(cols_max, c1 + dx_cells))
        return (new_c0, new_r0, new_c1, new_r1)

    def commit_spawn_zone_edit_drag(self) -> None:
        drag = self.spawn_zone_drag
        if drag is None:
            return
        candidate = self.spawn_zone_candidate_rect()
        self.spawn_zone_drag = None
        if candidate is None:
            return
        c0, r0, c1, r1 = candidate
        if (c0, r0, c1, r1) == zone_rect(drag.original_zone):
            return
        if not (0 <= drag.index < len(self.map_data[drag.list_name])):
            return
        after = copy.deepcopy(self.map_data)
        zone = after[drag.list_name][drag.index]
        zone["cols"] = [c0, c1]
        zone["rows"] = [r0, r1]
        self.apply_change("Edit Spawn Zone", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(drag.list_name, zone)

    def selected_spawn_zone_has_fields(self) -> bool:
        # Only actor zones carry editable per-zone configuration (kind/count).
        # Player zones are plain rectangles.
        ref = self.selected_spawn_zone_ref
        return ref is not None and ref.list_name == ACTOR_ZONE_LIST

    def edit_selected_spawn_zone_fields(self) -> None:
        zone = self.selected_spawn_zone()
        ref = self.selected_spawn_zone_ref
        if zone is None or ref is None:
            return
        if ref.list_name != ACTOR_ZONE_LIST:
            return
        result = self.prompt_for_actor_spawn_fields(zone["kind"], zone["count"])
        if result is None:
            return
        kind, count = result
        after = copy.deepcopy(self.map_data)
        if not (0 <= ref.index < len(after[ref.list_name])):
            return
        after[ref.list_name][ref.index]["kind"] = kind
        after[ref.list_name][ref.index]["count"] = count
        self.apply_change("Edit Actor Spawn Zone", after)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.selected_spawn_zone_ref = self._zone_ref_after_change(ref.list_name, after[ref.list_name][ref.index])

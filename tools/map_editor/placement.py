"""Placement and material-assignment actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    BARRIER_KIND_TABLE,
    COOKIE_ZONE_LIST,
    FACES,
    KEY_ZONE_LIST,
    MODE_RAMP_UP,
    PLAYER_ZONE_LIST,
    SPAWN_ZONE_LISTS,
)
from .dialogs import ActorSpawnFieldsDialog, BarrierKindDialog, MaterialAssignmentDialog
from .geometry import (
    normalized_wall,
    ramp_error,
    ramp_points_from_cells,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
    wall_segments_between,
    zone_intersects_rect,
)


class PlacementMixin:
    # === Placement (paint / draw new segments) ===

    def _face_materials_for_current(self) -> dict[str, str]:
        return {face: self.current_material for face in FACES}

    def _new_floor(self, col: int, row: int) -> dict:
        return {"col": col, "row": row, **self._face_materials_for_current()}

    def _new_wall(self, c0: int, r0: int, c1: int, r1: int) -> dict:
        return {"c0": c0, "r0": r0, "c1": c1, "r1": r1, **self._face_materials_for_current()}

    def _new_ramp(self, low: list[int], high: list[int], lower_level: int) -> dict:
        return {
            "low": low,
            "high": high,
            "lower_level": lower_level,
            **self._face_materials_for_current(),
        }

    def add_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        existing_floors = {(f["col"], f["row"]): f for f in level["floors"]}
        existing_inacc = {(f["col"], f["row"]): f for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                if (col, row) not in existing_floors:
                    existing_floors[(col, row)] = self._new_floor(col, row)
                existing_inacc.pop((col, row), None)
        level["floors"] = list(existing_floors.values())
        level["inaccessible_floors"] = list(existing_inacc.values())
        self.apply_change("Paint Floor", after)

    def add_inaccessible_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        existing_floors = {(f["col"], f["row"]): f for f in level["floors"]}
        existing_inacc = {(f["col"], f["row"]): f for f in level["inaccessible_floors"]}
        for row in range(r0, r1):
            for col in range(c0, c1):
                existing_floors.pop((col, row), None)
                if (col, row) not in existing_inacc:
                    existing_inacc[(col, row)] = self._new_floor(col, row)
        level["floors"] = list(existing_floors.values())
        level["inaccessible_floors"] = list(existing_inacc.values())
        # Drop any spawn zone whose rect intersects the new inaccessible-floor rect on this level.
        for list_name in SPAWN_ZONE_LISTS:
            after[list_name] = [
                zone
                for zone in after[list_name]
                if not (zone["level"] == self.current_level and zone_intersects_rect(zone, (c0, r0, c1, r1)))
            ]
        self.apply_change("Paint Inaccessible Floor", after)

    def add_actor_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        result = self.prompt_for_actor_spawn_fields()
        if result is None:
            return
        kind, count = result
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": count,
        }
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Actor Spawn Zone", after)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.selected_spawn_zone_ref = self._zone_ref_after_change(ACTOR_ZONE_LIST, new_zone)

    def add_player_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
        }
        after[PLAYER_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Player Spawn Zone", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(PLAYER_ZONE_LIST, new_zone)

    def add_cookie_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
        }
        after[COOKIE_ZONE_LIST].append(new_zone)
        self.apply_change("Paint Cookie Spawn Zone", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(COOKIE_ZONE_LIST, new_zone)

    def prompt_and_add_key_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = BarrierKindDialog.prompt(self, "Place Key Spawn Zone", self.recent_key_kind)
        if kind is None:
            return
        self.recent_key_kind = kind
        self.add_key_spawn_zone_rect(start, end, kind)

    def add_key_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in BARRIER_KIND_TABLE:
            self._flash_status(f"Unknown key kind {kind!r}")
            return
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
        }
        after[KEY_ZONE_LIST].append(new_zone)
        self.apply_change(f"Paint Key Spawn Zone ({kind})", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(KEY_ZONE_LIST, new_zone)

    def prompt_for_actor_spawn_fields(
        self,
        kind: str | None = None,
        count: int | None = None,
    ) -> tuple[str, int] | None:
        return ActorSpawnFieldsDialog.prompt(
            self,
            kind if kind is not None else self.recent_actor_spawn_kind,
            count if count is not None else self.recent_actor_spawn_count,
        )

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        edges = wall_segments_between(start, end)
        if not edges:
            return
        after = copy.deepcopy(self.map_data)
        existing = {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]])): w
            for w in after["levels"][self.current_level]["walls"]
        }
        for edge in edges:
            key = tuple(normalized_wall(edge))
            if key not in existing:
                c0, r0, c1, r1 = key
                existing[key] = self._new_wall(c0, r0, c1, r1)
        after["levels"][self.current_level]["walls"] = list(existing.values())
        # Painting a wall on an edge displaces any barrier on the same edge
        # (the loader rejects co-located walls + barriers).
        new_wall_keys = {key for key in existing.keys()}
        after["levels"][self.current_level]["barriers"] = [
            b for b in after["levels"][self.current_level].get("barriers", [])
            if tuple(normalized_wall([b["c0"], b["r0"], b["c1"], b["r1"]])) not in new_wall_keys
        ]
        self.apply_change("Place Wall", after)

    def prompt_and_add_barrier_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        """Prompt for kind via dialog (defaults to last-used), then place."""
        kind = BarrierKindDialog.prompt(self, "Place Barrier", self.recent_barrier_kind)
        if kind is None:
            return
        self.recent_barrier_kind = kind
        self.add_barrier_line(start, end, kind)

    def prompt_and_add_pressure_plate(self, col: int, row: int) -> None:
        """Place a pressure plate on (col, row) of the current level, after
        prompting for the barrier kind it controls. Last-used kind defaults."""
        kind = BarrierKindDialog.prompt(self, "Place Pressure Plate", self.recent_pressure_plate_kind)
        if kind is None:
            return
        self.recent_pressure_plate_kind = kind
        self.add_pressure_plate(col, row, kind)

    def add_pressure_plate(self, col: int, row: int, kind: str) -> None:
        if kind not in BARRIER_KIND_TABLE:
            self._flash_status(f"Unknown plate kind {kind!r}")
            return
        after = copy.deepcopy(self.map_data)
        plates = after.setdefault("pressure_plates", [])
        # Dedup on (level, col, row, kind) — same kind on same cell is a no-op.
        existing = {(p["level"], p["col"], p["row"], p["kind"]) for p in plates}
        new_plate = {"level": self.current_level, "col": col, "row": row, "kind": kind}
        if (new_plate["level"], new_plate["col"], new_plate["row"], new_plate["kind"]) in existing:
            return
        plates.append(new_plate)
        self.apply_change(f"Place Pressure Plate ({kind})", after)

    def remove_pressure_plate_at(self, col: int, row: int) -> bool:
        """Remove any plate of any kind at (current_level, col, row). Returns
        True if a plate was removed."""
        after = copy.deepcopy(self.map_data)
        plates = after.get("pressure_plates", [])
        keep = [p for p in plates if not (p["level"] == self.current_level and p["col"] == col and p["row"] == row)]
        if len(keep) == len(plates):
            return False
        after["pressure_plates"] = keep
        self.apply_change("Remove Pressure Plate", after)
        return True

    def add_barrier_line(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        edges = wall_segments_between(start, end)
        if not edges:
            return
        if kind not in BARRIER_KIND_TABLE:
            self._flash_status(f"Unknown barrier kind {kind!r}")
            return
        after = copy.deepcopy(self.map_data)
        level = after["levels"][self.current_level]
        wall_keys = {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]]))
            for w in level["walls"]
        }
        existing = {
            tuple(normalized_wall([b["c0"], b["r0"], b["c1"], b["r1"]])): b
            for b in level.get("barriers", [])
        }
        for edge in edges:
            key = tuple(normalized_wall(edge))
            if key in wall_keys:
                continue
            c0, r0, c1, r1 = key
            existing[key] = {"c0": c0, "r0": r0, "c1": c1, "r1": r1, "kind": kind}
        level["barriers"] = list(existing.values())
        self.apply_change(f"Place Barrier ({kind})", after)

    def add_ramp(self, start_cell: tuple[int, int], end_cell: tuple[int, int], mode: str) -> None:
        start_point, end_point = ramp_points_from_cells(start_cell, end_cell)
        if mode == MODE_RAMP_UP:
            if self.current_level + 1 >= len(self.map_data["levels"]):
                self._flash_status("Ramp not placed: Ramp (Up) needs an upper level")
                return
            lower_level = self.current_level
            low = start_point
            high = end_point
        else:
            if self.current_level == 0:
                self._flash_status("Ramp not placed: Ramp (Down) needs a lower level")
                return
            lower_level = self.current_level - 1
            low = end_point
            high = start_point

        msg = ramp_error(
            low,
            high,
            lower_level,
            self.map_data["grid_cols"],
            self.map_data["grid_rows"],
            len(self.map_data["levels"]),
        )
        if msg:
            self._flash_status(f"Ramp not placed: {msg}")
            return
        new_ramp = self._new_ramp(low, high, lower_level)
        new_rect = ramp_rect(new_ramp)
        after = copy.deepcopy(self.map_data)
        after["ramps"] = [
            ramp
            for ramp in after["ramps"]
            if self.current_level not in (ramp["lower_level"], ramp["lower_level"] + 1)
            or not rects_overlap(new_rect, ramp_rect(ramp))
        ]
        after["ramps"].append(new_ramp)
        self.apply_change(f"Place {mode}", after)

    # === Material assignment ===

    def assign_floor_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]

        def floor_in_rect(f: dict) -> bool:
            return c0 <= f["col"] < c1 and r0 <= f["row"] < r1

        affected_floors = [f for f in level["floors"] if floor_in_rect(f)] + [
            f for f in level["inaccessible_floors"] if floor_in_rect(f)
        ]
        if not affected_floors:
            self._flash_status("No floor segments in selection.")
            return
        seed = affected_floors[0]
        result = MaterialAssignmentDialog.prompt(
            self, "Floor Materials",
            f"{len(affected_floors)} floor cell(s) in selection",
            self.materials_catalog,
            {face: seed.get(face, self.current_material) for face in FACES},
        )
        if result is None:
            return
        after = copy.deepcopy(self.map_data)
        for floor in after["levels"][level_idx]["floors"]:
            if floor_in_rect(floor):
                floor.update(result)
        for floor in after["levels"][level_idx]["inaccessible_floors"]:
            if floor_in_rect(floor):
                floor.update(result)
        self.apply_change("Assign Floor Materials", after)

    def assign_wall_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        # Selection is a 2D rectangle defined by two grid points. A wall is
        # "in" the selection iff both endpoints lie inside the rect (so walls
        # only touching at a corner are not affected). A flat selection
        # (start and end share a row or column) collapses to a single grid
        # line — exactly the walls along that row/column.
        c0, c1 = sorted([start[0], end[0]])
        r0, r1 = sorted([start[1], end[1]])
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]

        def edge_inside(wall: dict) -> bool:
            return (
                c0 <= wall["c0"] <= c1
                and c0 <= wall["c1"] <= c1
                and r0 <= wall["r0"] <= r1
                and r0 <= wall["r1"] <= r1
            )

        affected_walls = [w for w in level["walls"] if edge_inside(w)]
        if not affected_walls:
            self._flash_status("No wall edges in selection.")
            return
        seed = affected_walls[0]
        result = MaterialAssignmentDialog.prompt(
            self, "Wall Materials",
            f"{len(affected_walls)} wall edge(s) in selection",
            self.materials_catalog,
            {face: seed.get(face, self.current_material) for face in FACES},
        )
        if result is None:
            return
        after = copy.deepcopy(self.map_data)
        for wall in after["levels"][level_idx]["walls"]:
            if edge_inside(wall):
                wall.update(result)
        self.apply_change("Assign Wall Materials", after)

    def assign_ramp_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        # Selection is a cell rect; any ramp whose footprint overlaps the rect
        # is in. Ramps live on the lower of the two levels they connect; only
        # those at the current level qualify.
        c0, r0, c1, r1 = rect_from_cells(start, end)
        level_idx = self.current_level

        def ramp_in_rect(ramp: dict) -> bool:
            return level_idx == ramp["lower_level"] and rects_overlap(
                (c0, r0, c1, r1), ramp_rect(ramp)
            )

        affected_ramps = [r for r in self.map_data["ramps"] if ramp_in_rect(r)]
        if not affected_ramps:
            self._flash_status("No ramps in selection.")
            return
        seed = affected_ramps[0]
        result = MaterialAssignmentDialog.prompt(
            self, "Ramp Materials",
            f"{len(affected_ramps)} ramp(s) in selection",
            self.materials_catalog,
            {face: seed.get(face, self.current_material) for face in FACES},
        )
        if result is None:
            return
        after = copy.deepcopy(self.map_data)
        for ramp in after["ramps"]:
            if ramp_in_rect(ramp):
                ramp.update(result)
        self.apply_change("Assign Ramp Materials", after)

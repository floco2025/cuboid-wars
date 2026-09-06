"""Placement and material-assignment actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    FACES,
    MODE_RAMP_UP,
    PLATE_TYPE_BARRIER,
    PLATE_TYPE_BRIDGE,
    PLATE_TYPE_FIREWORK,
    PLAYER_ZONE_LIST,
)
from .dialogs import ActorSpawnFieldsDialog, KindDialog, MaterialAssignmentDialog
from .editing import material_values, paint_bridges, paint_edges, paint_floors, paint_grass, place_plate, place_ramp, top_left_materials, update_records
from .normalization import pressure_plate_key
from .geometry import (
    ramp_error,
    ramp_points_from_cells,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
)


class PlacementMixin:
    def placement_kind(self, title: str, kinds: list[str], recent: str | None, noun: str) -> str | None:
        if recent in kinds:
            return recent
        return KindDialog.prompt(self, title, kinds, recent, noun)

    # === Placement (paint / draw new segments) ===

    def _new_ramp(self, low: list[int], high: list[int], lower_level: int) -> dict:
        return {"low": low, "high": high, "lower_level": lower_level, **dict.fromkeys(FACES, self.current_material)}

    def add_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Paint Floor", paint_floors(self.map_data, self.current_level, rect_from_cells(start, end), self.current_material))

    def add_inaccessible_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Paint Inaccessible Floor", paint_floors(self.map_data, self.current_level, rect_from_cells(start, end), self.current_material, blocked=True))

    def add_grass_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Paint Grass", paint_grass(self.map_data, self.current_level, rect_from_cells(start, end)))

    def add_actor_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        result = self.prompt_for_actor_spawn_fields()
        if result is None:
            return
        kind, count = result
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": count,
        }
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.apply_change("Paint Actor Spawn Zone", after)
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

    def prompt_for_actor_spawn_fields(
        self,
        kind: str | None = None,
        count: int | None = None,
    ) -> tuple[str, int] | None:
        if kind is None and self.recent_actor_spawn_kind in self.actor_kinds:
            return self.recent_actor_spawn_kind, self.recent_actor_spawn_count
        return ActorSpawnFieldsDialog.prompt(
            self,
            kind if kind is not None else self.recent_actor_spawn_kind,
            count if count is not None else self.recent_actor_spawn_count,
        )

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Place Wall", paint_edges(self.map_data, self.current_level, start, end, material=self.current_material))

    def prompt_and_add_barrier_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind("Place Barrier", self.barrier_kinds, self.recent_barrier_kind, "barrier")
        if kind is None:
            return
        self.recent_barrier_kind = kind
        self.add_barrier_line(start, end, kind)

    def prompt_and_add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind("Place Light Bridge", self.bridge_kinds, self.recent_bridge_kind, "bridge")
        if kind is None:
            return
        self.recent_bridge_kind = kind
        self.add_light_bridge_rect(start, end, kind)

    def add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in self.bridge_kinds:
            self.notify(f"Unknown bridge kind {kind!r}")
            return
        self.apply_change(f"Place Light Bridge ({kind})", paint_bridges(self.map_data, self.current_level, rect_from_cells(start, end), kind))

    def prompt_and_add_pressure_plate(self, col: int, row: int) -> None:
        kind = self.placement_kind(
            "Place Barrier Plate", self.barrier_kinds, self.recent_pressure_plate_kind, "barrier"
        )
        if kind is None:
            return
        self.recent_pressure_plate_kind = kind
        self.add_pressure_plate(col, row, kind)

    def add_pressure_plate(self, col: int, row: int, kind: str) -> None:
        if kind not in self.barrier_kinds:
            self.notify(f"Unknown plate kind {kind!r}")
            return
        plate = {"level": self.current_level, "col": col, "row": row, "type": PLATE_TYPE_BARRIER, "kind": kind}
        self._add_plate(plate, f"Place Barrier Plate ({kind})")

    def prompt_and_add_bridge_plate(self, col: int, row: int) -> None:
        kind = self.placement_kind(
            "Place Bridge Plate", self.bridge_kinds, self.recent_bridge_plate_kind, "bridge"
        )
        if kind is None:
            return
        self.recent_bridge_plate_kind = kind
        self.add_bridge_plate(col, row, kind)

    def add_bridge_plate(self, col: int, row: int, kind: str) -> None:
        if kind not in self.bridge_kinds:
            self.notify(f"Unknown plate kind {kind!r}")
            return
        plate = {"level": self.current_level, "col": col, "row": row, "type": PLATE_TYPE_BRIDGE, "kind": kind}
        self._add_plate(plate, f"Place Bridge Plate ({kind})")

    def add_firework_plate(self, col: int, row: int) -> None:
        plate = {"level": self.current_level, "col": col, "row": row, "type": PLATE_TYPE_FIREWORK}
        self._add_plate(plate, "Place Firework Plate")

    def plates_at(self, col: int, row: int) -> list[dict]:
        return [
            plate
            for plate in self.map_data.get("pressure_plates", [])
            if plate["level"] == self.current_level and (plate["col"], plate["row"]) == (col, row)
        ]

    def edit_pressure_plate_at(self, key: tuple) -> None:
        plate = next((p for p in self.map_data["pressure_plates"] if pressure_plate_key(p) == key), None)
        if plate is None or plate["type"] == PLATE_TYPE_FIREWORK:
            return
        barrier = plate["type"] == PLATE_TYPE_BARRIER
        kinds, noun = (self.barrier_kinds, "barrier") if barrier else (self.bridge_kinds, "bridge")
        title = f"Edit {noun.capitalize()} Plate"
        kind = KindDialog.prompt(self, title, kinds, plate["kind"], noun)
        if kind is None or kind == plate["kind"]:
            return
        try:
            after = place_plate(self.map_data, {**plate, "kind": kind}, replacing=key)
        except ValueError as exc:
            self.notify(str(exc))
            return
        self.apply_change(title, after)

    def _add_plate(self, plate: dict, label: str) -> None:
        try:
            after = place_plate(self.map_data, plate)
        except ValueError as exc:
            self.notify(f"Plate not placed: {exc}")
            return
        self.apply_change(label, after)

    def erase_pressure_plate(self, key: tuple) -> None:
        after = copy.deepcopy(self.map_data)
        after["pressure_plates"] = [p for p in after["pressure_plates"] if pressure_plate_key(p) != key]
        self.apply_change("Erase Pressure Plate", after)

    def add_barrier_line(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in self.barrier_kinds:
            self.notify(f"Unknown barrier kind {kind!r}")
            return
        self.apply_change(f"Place Barrier ({kind})", paint_edges(self.map_data, self.current_level, start, end, kind=kind))

    def add_ramp(self, start_cell: tuple[int, int], end_cell: tuple[int, int], mode: str) -> None:
        start_point, end_point = ramp_points_from_cells(start_cell, end_cell)
        if mode == MODE_RAMP_UP:
            if self.current_level + 1 >= len(self.map_data["levels"]):
                self.notify("Ramp not placed: Ramp (Up) needs an upper level")
                return
            lower_level = self.current_level
            low = start_point
            high = end_point
        else:
            if self.current_level == 0:
                self.notify("Ramp not placed: Ramp (Down) needs a lower level")
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
            self.notify(f"Ramp not placed: {msg}")
            return
        new_ramp = self._new_ramp(low, high, lower_level)
        self.apply_change(f"Place {mode}", place_ramp(self.map_data, new_ramp))

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
            self.notify("No floor segments in selection.")
            return
        result = MaterialAssignmentDialog.prompt(
            self, "Floor Materials",
            f"{len(affected_floors)} floor cell(s) in selection",
            self.materials_catalog,
            material_values(affected_floors),
            source=top_left_materials(affected_floors, "floors"),
        )
        if result is None:
            return
        after = update_records(self.map_data, "floors", floor_in_rect, result, level_idx)
        after = update_records(after, "inaccessible_floors", floor_in_rect, result, level_idx)
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
            self.notify("No wall edges in selection.")
            return
        result = MaterialAssignmentDialog.prompt(
            self, "Wall Materials",
            f"{len(affected_walls)} wall edge(s) in selection",
            self.materials_catalog,
            material_values(affected_walls),
            source=top_left_materials(affected_walls, "walls"),
        )
        if result is None:
            return
        after = update_records(self.map_data, "walls", edge_inside, result, level_idx)
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
            self.notify("No ramps in selection.")
            return
        result = MaterialAssignmentDialog.prompt(
            self, "Ramp Materials",
            f"{len(affected_ramps)} ramp(s) in selection",
            self.materials_catalog,
            material_values(affected_ramps),
            source=top_left_materials(affected_ramps, "ramps"),
        )
        if result is None:
            return
        after = update_records(self.map_data, "ramps", ramp_in_rect, result)
        self.apply_change("Assign Ramp Materials", after)

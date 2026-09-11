"""Placement and material-assignment actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    CHECKPOINT_LIST,
    FACES,
    HIT_FLOOR,
    HIT_INACCESSIBLE_FLOOR,
    HIT_RAMP,
    HIT_WALL,
    MODE_RAMP_UP,
    PLAYER_ZONE_LIST,
)
from .dialogs import ActorSpawnFieldsDialog, KindDialog, MaterialAssignmentDialog
from .dialogs.controls import FieldPropertiesDialog
from .editing import (
    material_values,
    merge_record,
    paint_bridges,
    paint_edges,
    paint_erasers,
    paint_floors,
    paint_grass,
    place_plate,
    place_ramp,
    top_left_materials,
    update_records,
)
from .normalization import edge_key, plate_cell_error, pressure_plate_key
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
        kind, count, switch, inverted = result
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": count,
        }
        if switch:
            new_zone["switch"] = switch
            new_zone["switch_inverted"] = inverted
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.recent_actor_spawn_switch = switch or ""
        self.recent_actor_spawn_inverted = inverted
        self.apply_change("Paint Actor Spawn Zone", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(ACTOR_ZONE_LIST, new_zone)

    def add_player_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self._add_zone_rect(PLAYER_ZONE_LIST, "Player Spawn Zone", start, end)

    def add_checkpoint_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self._add_zone_rect(CHECKPOINT_LIST, "Checkpoint", start, end)

    def _add_zone_rect(self, list_name: str, label: str, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": self.current_level,
            "cols": [c0, c1],
            "rows": [r0, r1],
        }
        if list_name == CHECKPOINT_LIST:
            new_zone["type"] = self.recent_checkpoint_type
        after[list_name].append(new_zone)
        self.apply_change(f"Paint {label}", after)
        self.selected_spawn_zone_ref = self._zone_ref_after_change(list_name, new_zone)

    def prompt_for_actor_spawn_fields(
        self,
        kind: str | None = None,
        count: int | None = None,
        switch: str | None = None,
        inverted: bool = False,
    ) -> tuple[str, int, str | None, bool] | None:
        if kind is None and self.recent_actor_spawn_kind in self.actor_kinds:
            recent_switch = self.recent_actor_spawn_switch
            return self.recent_actor_spawn_kind, self.recent_actor_spawn_count, recent_switch or None, self.recent_actor_spawn_inverted
        return ActorSpawnFieldsDialog.prompt(
            self,
            kind if kind is not None else self.recent_actor_spawn_kind,
            count if count is not None else self.recent_actor_spawn_count,
            self.switches,
            switch if kind is not None else (self.recent_actor_spawn_switch or None),
            inverted if kind is not None else self.recent_actor_spawn_inverted,
        )

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Place Wall", paint_edges(self.map_data, self.current_level, start, end, material=self.current_material))

    def add_equipment_eraser_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Place Equipment Eraser", paint_erasers(self.map_data, self.current_level, start, end))

    def prompt_and_add_barrier_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind("Place Barrier", self.barrier_kinds, self.recent_barrier_kind, "barrier kind")
        if kind is None:
            return
        self.recent_barrier_kind = kind
        self.add_barrier_line(start, end, kind)

    def prompt_and_add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind("Place Light Bridge", self.bridge_kinds, self.recent_bridge_kind, "bridge kind")
        if kind is None:
            return
        self.recent_bridge_kind = kind
        self.add_light_bridge_rect(start, end, kind)

    def add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in self.bridge_kinds:
            self.notify(f"Unknown bridge kind {kind!r}")
            return
        self.apply_change(f"Place Light Bridge ({kind})", paint_bridges(self.map_data, self.current_level, rect_from_cells(start, end), kind, self.recent_bridge_controls))

    def edit_light_bridge_at(self, col: int, row: int) -> None:
        def matches(bridge: dict) -> bool:
            return (bridge["col"], bridge["row"]) == (col, row)

        bridge = next((b for b in self.map_data["levels"][self.current_level]["light_bridges"] if matches(b)), None)
        if bridge is None:
            return
        self.edit_fields("light_bridges", "Edit Light Bridge", matches)

    def edit_fields(self, name: str, title: str, matches) -> None:
        entries = [entry for entry in self.map_data["levels"][self.current_level][name] if matches(entry)]
        if not entries:
            return
        barrier = name == "barriers"
        values = FieldPropertiesDialog.prompt(self, title, self.barrier_kinds if barrier else self.bridge_kinds,
                                              self.switches, entries)
        if values is not None:
            self.apply_change(title, update_records(self.map_data, name, matches, values, self.current_level))

    def edit_selected_fields(self, name: str) -> None:
        if self.tile_selection is None:
            return
        c0, r0, c1, r1 = self.tile_selection
        def matches(entry):
            if name == "barriers":
                return c0 <= entry["c0"] <= c1 and c0 <= entry["c1"] <= c1 and r0 <= entry["r0"] <= r1 and r0 <= entry["r1"] <= r1
            return c0 <= entry["col"] < c1 and r0 <= entry["row"] < r1
        self.edit_fields(name, "Edit Selected Barriers" if name == "barriers" else "Edit Selected Light Bridges", matches)

    def configure_field_defaults(self, barrier: bool) -> None:
        prefix = "barrier" if barrier else "bridge"
        controls = getattr(self, f"recent_{prefix}_controls")
        kind = getattr(self, f"recent_{prefix}_kind")
        values = FieldPropertiesDialog.prompt(self, "Barrier Defaults" if barrier else "Light Bridge Defaults",
                                              self.barrier_kinds if barrier else self.bridge_kinds,
                                              self.switches,
                                              [{"kind": kind, **controls}])
        if values is not None:
            defaults = merge_record({"kind": kind, **controls}, values)
            setattr(self, f"recent_{prefix}_kind", defaults.pop("kind", kind))
            setattr(self, f"recent_{prefix}_controls", defaults)
            self.tool_settings.refresh()

    def prompt_and_add_pressure_plate(self, col: int, row: int) -> None:
        switch = self.placement_kind("Place Pressure Plate", self.switches, self.recent_pressure_plate_switch, "switch")
        if switch is None:
            return
        self.recent_pressure_plate_switch = switch
        self.add_pressure_plate(col, row, switch)

    def add_pressure_plate(self, col: int, row: int, switch: str) -> None:
        if switch not in self.switches:
            self.notify(f"Unknown switch {switch!r}")
            return
        plate = {"level": self.current_level, "col": col, "row": row, "switch": switch}
        self._add_plate(plate, f"Place Pressure Plate ({switch})")

    def plates_at(self, col: int, row: int) -> list[dict]:
        return [
            plate
            for plate in self.map_data.get("pressure_plates", [])
            if plate["level"] == self.current_level and (plate["col"], plate["row"]) == (col, row)
        ]

    def edit_pressure_plate_at(self, key: tuple) -> None:
        plate = next((p for p in self.map_data["pressure_plates"] if pressure_plate_key(p) == key), None)
        if plate is None:
            return
        title = "Edit Pressure Plate"
        switch = KindDialog.prompt(self, title, self.switches, plate.get("switch"), "switch")
        if switch is None or switch == plate.get("switch"):
            return
        try:
            after = place_plate(self.map_data, {**plate, "switch": switch}, replacing=key)
        except ValueError as exc:
            self.notify(str(exc))
            return
        self.apply_change(title, after)

    def _add_plate(self, plate: dict, label: str) -> None:
        error = plate_cell_error(self.map_data, plate["level"], plate["col"], plate["row"])
        if error is not None:
            self.notify(f"Plate not placed: cell {error}.")
            return
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
        self.apply_change(f"Place Barrier ({kind})", paint_edges(self.map_data, self.current_level, start, end, kind=kind, controls=self.recent_barrier_controls))

    def edit_barrier_at(self, key: tuple) -> None:
        barrier = next((b for b in self.map_data["levels"][self.current_level]["barriers"] if edge_key(b) == key), None)
        if barrier is None:
            return
        self.edit_fields("barriers", "Edit Barrier", lambda b: edge_key(b) == key)

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

    def edit_materials_at(self, hit) -> None:
        kind, key = hit
        level = self.current_level
        if kind in (HIT_FLOOR, HIT_INACCESSIBLE_FLOOR):
            name = "floors" if kind == HIT_FLOOR else "inaccessible_floors"
            matches = lambda entry: (entry["col"], entry["row"]) == key
        elif kind == HIT_WALL:
            name = "walls"
            matches = lambda entry: edge_key(entry) == key
        elif kind == HIT_RAMP:
            name, level = "ramps", None
            matches = lambda entry: (entry["lower_level"], tuple(entry["low"]), tuple(entry["high"])) == key
        else:
            return
        target = self.map_data if level is None else self.map_data["levels"][level]
        entry = next((entry for entry in target[name] if matches(entry)), None)
        if entry is None:
            return
        title = f"Edit {kind} Materials"
        result = MaterialAssignmentDialog.prompt(
            self, title, f"1 {kind.lower()}", self.materials_catalog, material_values([entry]),
            portalability=self.texture_catalog,
            source=top_left_materials([entry], name),
        )
        if result is not None:
            self.apply_change(title, update_records(self.map_data, name, matches, result, level))

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
            portalability=self.texture_catalog,
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
            portalability=self.texture_catalog,
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
            portalability=self.texture_catalog,
            source=top_left_materials(affected_ramps, "ramps"),
        )
        if result is None:
            return
        after = update_records(self.map_data, "ramps", ramp_in_rect, result)
        self.apply_change("Assign Ramp Materials", after)

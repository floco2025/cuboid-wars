"""Placement and material-assignment actions for the editor window."""

from __future__ import annotations

import copy

from .constants import (
    ACTOR_ZONE_LIST,
    CHECKPOINT_LIST,
    MODE_RAMP_UP,
    PLAYER_ZONE_LIST,
)
from .dialogs import ActorSpawnFieldsDialog, KindDialog
from .dialogs.controls import FieldPropertiesDialog
from .editing import (
    placement_materials,
    merge_record,
    paint_bridges,
    paint_edges,
    paint_erasers,
    paint_floors,
    paint_terrain,
    place_plate,
    place_ramp,
)
from .normalization import plate_cell_error
from .geometry import (
    ramp_error,
    ramp_points_from_cells,
    ramp_rect,
    rect_from_cells,
    rects_overlap,
)


class PlacementMixin:
    def placement_kind(self, title: str, kinds: list[str], recent: str | None, noun: str, colors=None) -> str | None:
        if recent in kinds:
            return recent
        return KindDialog.prompt(self, title, kinds, recent, noun, colors)

    # === Placement (paint / draw new segments) ===

    def placement_material(self):
        return self.sampled_materials if self.sampled_materials is not None else self.current_material

    def _new_ramp(self, low: list[int], high: list[int], lower_level: int) -> dict:
        return {"low": low, "high": high, "lower_level": lower_level, **placement_materials(self.placement_material())}

    def add_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change(
            "Paint Floor",
            paint_floors(self.map_data, self.current_level, rect_from_cells(start, end), self.placement_material()),
        )

    def add_inaccessible_floor_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change(
            "Paint Inaccessible Floor",
            paint_floors(
                self.map_data, self.current_level, rect_from_cells(start, end), self.placement_material(), blocked=True
            ),
        )

    def add_terrain_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change(
            "Paint Terrain",
            paint_terrain(
                self.map_data,
                self.current_level,
                rect_from_cells(start, end),
                self.placement_material(),
            ),
        )

    def add_actor_spawn_zone_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        result = self.prompt_for_actor_spawn_fields()
        if result is None:
            return
        kind, count, respawn_secs, switch, inverted, level, levels, roam_distance = result
        c0, r0, c1, r1 = rect_from_cells(start, end)
        after = copy.deepcopy(self.map_data)
        new_zone = {
            "level": level,
            "levels": levels,
            "roam_distance": roam_distance,
            "cols": [c0, c1],
            "rows": [r0, r1],
            "kind": kind,
            "count": count,
            "respawn_secs": respawn_secs,
        }
        if switch:
            new_zone["switch"] = switch
            new_zone["switch_inverted"] = inverted
        after[ACTOR_ZONE_LIST].append(new_zone)
        self.recent_actor_spawn_kind = kind
        self.recent_actor_spawn_count = count
        self.recent_actor_spawn_respawn_secs = respawn_secs
        self.recent_actor_spawn_switch = switch or ""
        self.recent_actor_spawn_inverted = inverted
        self.recent_actor_spawn_levels = levels
        self.recent_actor_roam_distance = roam_distance
        self.apply_change("Paint Actor Spawn Zone", after)

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
            name = self.recent_checkpoint_name.strip()
            if name:
                if any(zone.get("name") == name for zone in self.map_data[CHECKPOINT_LIST]):
                    self.notify(f"Checkpoint name {name!r} is already in use. Choose another name or leave it blank.")
                    return
                new_zone["name"] = name
        else:
            new_zone["levels"] = min(self.recent_player_spawn_levels, len(self.map_data["levels"]) - self.current_level)
        after[list_name].append(new_zone)
        self.apply_change(f"Paint {label}", after)

    # Without `kind` the toolbar's recent values stand for a new zone; with it
    # every argument is the edited zone's own, `respawn_secs` None included.
    def prompt_for_actor_spawn_fields(
        self,
        kind: str | None = None,
        count: list[int] | None = None,
        respawn_secs: int | None = None,
        switch: str | None = None,
        inverted: bool = False,
        level=None,
        levels=None,
        roam_distance=None,
    ):
        if kind is None and self.recent_actor_spawn_kind in self.actor_kinds:
            recent_switch = self.recent_actor_spawn_switch
            return (
                self.recent_actor_spawn_kind,
                self.recent_actor_spawn_count,
                self.recent_actor_spawn_respawn_secs,
                recent_switch or None,
                self.recent_actor_spawn_inverted,
                self.current_level,
                min(self.recent_actor_spawn_levels, len(self.map_data["levels"]) - self.current_level),
                self.recent_actor_roam_distance,
            )
        return ActorSpawnFieldsDialog.prompt(
            self,
            kind if kind is not None else self.recent_actor_spawn_kind,
            count if count is not None else self.recent_actor_spawn_count,
            respawn_secs if kind is not None else self.recent_actor_spawn_respawn_secs,
            self.switches,
            switch if kind is not None else (self.recent_actor_spawn_switch or None),
            inverted if kind is not None else self.recent_actor_spawn_inverted,
            level=self.current_level if level is None else level,
            levels=self.recent_actor_spawn_levels if levels is None else levels,
            roam_distance=self.recent_actor_roam_distance if roam_distance is None else roam_distance,
            level_names=[entry.get("name", f"Level {index}") for index, entry in enumerate(self.map_data["levels"])],
        )

    def add_wall_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change(
            "Place Wall", paint_edges(self.map_data, self.current_level, start, end, material=self.placement_material())
        )

    def add_equipment_eraser_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        self.apply_change("Place Equipment Eraser", paint_erasers(self.map_data, self.current_level, start, end))

    def prompt_and_add_barrier_line(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind(
            "Place Barrier", self.barrier_kinds, self.recent_barrier_kind, "barrier kind", self.barrier_kind_colors
        )
        if kind is None:
            return
        self.recent_barrier_kind = kind
        self.add_barrier_line(start, end, kind)

    def prompt_and_add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        kind = self.placement_kind(
            "Place Light Bridge", self.bridge_kinds, self.recent_bridge_kind, "bridge kind", self.bridge_kind_colors
        )
        if kind is None:
            return
        self.recent_bridge_kind = kind
        self.add_light_bridge_rect(start, end, kind)

    def add_light_bridge_rect(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in self.bridge_kinds:
            self.notify(f"Unknown bridge kind {kind!r}")
            return
        self.apply_change(
            f"Place Light Bridge ({kind})",
            paint_bridges(
                self.map_data, self.current_level, rect_from_cells(start, end), kind, self.recent_bridge_controls
            ),
        )

    def configure_field_defaults(self, barrier: bool) -> None:
        prefix = "barrier" if barrier else "bridge"
        controls = getattr(self, f"recent_{prefix}_controls")
        kind = getattr(self, f"recent_{prefix}_kind")
        values = FieldPropertiesDialog.prompt(
            self,
            "Barrier Defaults" if barrier else "Light Bridge Defaults",
            self.barrier_kinds if barrier else self.bridge_kinds,
            self.switches,
            [{"kind": kind, **controls}],
            kind_colors=self.barrier_kind_colors if barrier else self.bridge_kind_colors,
            switch_colors=self.plate_colors,
        )
        if values is not None:
            defaults = merge_record({"kind": kind, **controls}, values)
            setattr(self, f"recent_{prefix}_kind", defaults.pop("kind", kind))
            setattr(self, f"recent_{prefix}_controls", defaults)
            self.tool_settings.refresh()

    def prompt_and_add_pressure_plate(self, col: int, row: int) -> None:
        switch = self.placement_kind(
            "Place Pressure Plate", self.switches, self.recent_pressure_plate_switch, "switch", self.plate_colors
        )
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

    def add_barrier_line(self, start: tuple[int, int], end: tuple[int, int], kind: str) -> None:
        if kind not in self.barrier_kinds:
            self.notify(f"Unknown barrier kind {kind!r}")
            return
        self.apply_change(
            f"Place Barrier ({kind})",
            paint_edges(
                self.map_data, self.current_level, start, end, kind=kind, controls=self.recent_barrier_controls
            ),
        )

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
        self.inspect_hit(hit, show=True)

    def assign_floor_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        self.open_properties_for(
            ("floors", "inaccessible_floors"), lambda entry: c0 <= entry["col"] < c1 and r0 <= entry["row"] < r1
        )

    def assign_wall_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, c1 = sorted((start[0], end[0]))
        r0, r1 = sorted((start[1], end[1]))
        self.open_properties_for(
            "walls",
            lambda entry: (
                c0 <= entry["c0"] <= c1
                and c0 <= entry["c1"] <= c1
                and r0 <= entry["r0"] <= r1
                and r0 <= entry["r1"] <= r1
            ),
        )

    def assign_ramp_materials_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        self.open_properties_for(
            "ramps",
            lambda entry: (
                self.current_level in (entry["lower_level"], entry["lower_level"] + 1)
                and rects_overlap(rect, ramp_rect(entry))
            ),
        )

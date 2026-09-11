"""Light placement actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QMessageBox

from .dialogs import AutoPlaceLightsDialog, KindDialog
from .editing import update_records
from .geometry import cell_side_from_click, ramp_cells_on_level, wall_endpoints_for_cell_side
from .normalization import edge_key, level_label, light_key, light_placement_error


class LightsMixin:
    # === Lights ===

    def edit_light_at(self, col: int, row: int, side: str) -> None:
        def matches(light: dict) -> bool:
            return (light["col"], light["row"], light["side"]) == (col, row, side)

        light = next((light for light in self.map_data["levels"][self.current_level]["lights"] if matches(light)), None)
        if light is None:
            return
        title = "Edit Light"
        kind = KindDialog.prompt(self, title, self.wall_light_kinds, light.get("kind"), "style")
        if kind is None or kind == light.get("kind"):
            return
        after = update_records(self.map_data, "lights", matches, {"kind": kind}, self.current_level)
        self.apply_change(title, after)

    def add_light_at(self, pos) -> None:
        px = pos.x()
        py = pos.y()
        cols = self.map_data["grid_cols"]
        rows = self.map_data["grid_rows"]
        col = int(px)
        row = int(py)
        if not (0 <= col < cols and 0 <= row < rows):
            return
        side = cell_side_from_click(col, row, px, py)
        level_idx = self.current_level
        error = light_placement_error(self.map_data, level_idx, col, row, side)
        if error is not None:
            self.notify(error)
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][level_idx]["lights"].append({"col": col, "row": row, "side": side, "kind": self.recent_light_kind})
        self.apply_change("Add Light", after)

    def auto_place_lights_on_current_level(
        self,
        row_spacing: int,
        row_offset: int,
        col_spacing: int,
        col_offset: int,
    ) -> None:
        self.recent_auto_place_lights = (row_spacing, row_offset, col_spacing, col_offset)
        cols = self.map_data["grid_cols"]
        rows = self.map_data["grid_rows"]
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]
        floors_on_level = {(f["col"], f["row"]) for f in level["floors"]}
        ramp_cells = ramp_cells_on_level(self.map_data["ramps"], level_idx)
        wall_set = {edge_key(w) for w in level["walls"]}
        # Column spacing controls placement *along* a horizontal wall (i.e.,
        # which X-positions get N/S lights). Row spacing controls placement
        # along a vertical wall (which Z-positions get E/W lights). The
        # python step is `spacing + 1`: spacing=0 → every cell, spacing=1 →
        # every other, spacing=2 → every third, etc.
        selected_cols = set(range(col_offset, cols, col_spacing + 1))
        selected_rows = set(range(row_offset, rows, row_spacing + 1))

        candidates: list[dict] = []
        for (c, r) in floors_on_level:
            if (c, r) in ramp_cells:
                continue
            if c in selected_cols:
                for side in ("N", "S"):
                    if wall_endpoints_for_cell_side(c, r, side) in wall_set:
                        candidates.append({"col": c, "row": r, "side": side, "kind": self.recent_light_kind})
            if r in selected_rows:
                for side in ("E", "W"):
                    if wall_endpoints_for_cell_side(c, r, side) in wall_set:
                        candidates.append({"col": c, "row": r, "side": side, "kind": self.recent_light_kind})

        if not candidates:
            self.notify("Auto-Place Lights: no walls matched the stride.")
            return

        after = copy.deepcopy(self.map_data)
        existing = after["levels"][level_idx]["lights"]
        existing_keys = {light_key(l) for l in existing}
        # Filter the candidate set down to "actually new" — the user's
        # preview only shows lights that would be added, not duplicates of
        # ones already on the level.
        new_lights = [c for c in candidates if light_key(c) not in existing_keys]
        if not new_lights:
            self.notify("Auto-Place Lights: nothing new to add.")
            return

        # Ghost preview: stash the candidate list, repaint the canvas (so
        # the user sees where the lights would land), then confirm. Cancel
        # leaves the level untouched; accept commits one undoable batch.
        self.pending_auto_lights = (level_idx, new_lights)
        self.canvas.update()
        response = QMessageBox.question(
            self,
            "Auto-Place Lights",
            f"Add {len(new_lights)} {self.recent_light_kind} light(s) to the highlighted positions?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Yes,
        )
        self.pending_auto_lights = None
        self.canvas.update()
        if response != QMessageBox.StandardButton.Yes:
            self.notify("Auto-Place Lights: cancelled.")
            return
        for candidate in new_lights:
            existing.append(candidate)
        self.apply_change("Auto-Place Lights", after)
        self.notify(f"Auto-Place Lights: added {len(new_lights)} light(s).")

    def open_auto_place_lights_dialog(self) -> None:
        result = AutoPlaceLightsDialog.prompt(
            self,
            self.map_data["grid_cols"],
            self.map_data["grid_rows"],
            initial=self.recent_auto_place_lights,
            kinds=self.wall_light_kinds,
            initial_kind=self.recent_light_kind,
        )
        if result is None:
            return
        row_spacing, row_offset, col_spacing, col_offset, kind = result
        self.recent_light_kind = kind
        self.tool_settings.refresh()
        self.auto_place_lights_on_current_level(row_spacing, row_offset, col_spacing, col_offset)

    def clear_lights_on_current_level(self) -> None:
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]
        light_count = len(level["lights"])
        if light_count == 0:
            self.notify("Clear Lights: this level has no lights.")
            return
        # Wiping every light on a level is one menu click away — sanity-prompt
        # in line with Remove Level. Undo recovers but the modal makes the
        # action's blast radius visible.
        response = QMessageBox.question(
            self,
            "Clear Lights",
            f"Remove all {light_count} light(s) from {level_label(level, level_idx)}?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        if response != QMessageBox.StandardButton.Yes:
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][level_idx]["lights"] = []
        self.apply_change("Clear Lights", after)

"""Light placement actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QMessageBox

from .dialogs import AutoPlaceLightsDialog
from .geometry import (
    cell_side_from_click,
    level_label,
    light_key,
    normalized_wall,
    ramp_cells,
    rect_from_cells,
    wall_endpoints_for_cell_side,
)


class LightsMixin:
    # === Lights ===

    def _ramp_cells_for_level(self, level_idx: int) -> set[tuple[int, int]]:
        cells: set[tuple[int, int]] = set()
        for ramp in self.map_data["ramps"]:
            if level_idx in (ramp["lower_level"], ramp["lower_level"] + 1):
                cells.update(ramp_cells(ramp))
        return cells

    def _wall_endpoints_for_level(self, level_idx: int) -> set[tuple[int, int, int, int]]:
        return {
            tuple(normalized_wall([w["c0"], w["r0"], w["c1"], w["r1"]]))
            for w in self.map_data["levels"][level_idx]["walls"]
        }

    def toggle_light_at(self, pos, cell_size: float) -> None:
        px = pos.x() / cell_size
        py = pos.y() / cell_size
        cols = self.map_data["grid_cols"]
        rows = self.map_data["grid_rows"]
        col = int(px)
        row = int(py)
        if not (0 <= col < cols and 0 <= row < rows):
            return
        side = cell_side_from_click(col, row, px, py)
        level_idx = self.current_level
        endpoints = wall_endpoints_for_cell_side(col, row, side)
        if endpoints not in self._wall_endpoints_for_level(level_idx):
            self._flash_status(f"No wall on the {side} side of cell [{col}, {row}].")
            return
        if (col, row) in self._ramp_cells_for_level(level_idx):
            self._flash_status(f"Cannot place a light inside a ramp footprint ([{col}, {row}]).")
            return
        after = copy.deepcopy(self.map_data)
        lights = after["levels"][level_idx]["lights"]
        new_light = {"col": col, "row": row, "side": side}
        key = light_key(new_light)
        existing_idx = next((i for i, l in enumerate(lights) if light_key(l) == key), None)
        if existing_idx is not None:
            del lights[existing_idx]
            label = "Remove Light"
        else:
            lights.append(new_light)
            label = "Add Light"
        self.apply_change(label, after)

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
        ramp_cells_on_level = self._ramp_cells_for_level(level_idx)
        wall_set = self._wall_endpoints_for_level(level_idx)
        # Column spacing controls placement *along* a horizontal wall (i.e.,
        # which X-positions get N/S lights). Row spacing controls placement
        # along a vertical wall (which Z-positions get E/W lights). The
        # python step is `spacing + 1`: spacing=0 → every cell, spacing=1 →
        # every other, spacing=2 → every third, etc.
        selected_cols = set(range(col_offset, cols, col_spacing + 1))
        selected_rows = set(range(row_offset, rows, row_spacing + 1))

        candidates: list[dict] = []
        for (c, r) in floors_on_level:
            if (c, r) in ramp_cells_on_level:
                continue
            if c in selected_cols:
                for side in ("N", "S"):
                    if wall_endpoints_for_cell_side(c, r, side) in wall_set:
                        candidates.append({"col": c, "row": r, "side": side})
            if r in selected_rows:
                for side in ("E", "W"):
                    if wall_endpoints_for_cell_side(c, r, side) in wall_set:
                        candidates.append({"col": c, "row": r, "side": side})

        if not candidates:
            self._flash_status("Auto-Place Lights: no walls matched the stride.")
            return

        after = copy.deepcopy(self.map_data)
        existing = after["levels"][level_idx]["lights"]
        existing_keys = {light_key(l) for l in existing}
        # Filter the candidate set down to "actually new" — the user's
        # preview only shows lights that would be added, not duplicates of
        # ones already on the level.
        new_lights = [c for c in candidates if light_key(c) not in existing_keys]
        if not new_lights:
            self._flash_status("Auto-Place Lights: nothing new to add.")
            return

        # Ghost preview: stash the candidate list, repaint the canvas (so
        # the user sees where the lights would land), then confirm. Cancel
        # leaves the level untouched; accept commits one undoable batch.
        self.pending_auto_lights = (level_idx, new_lights)
        self.canvas.update()
        response = QMessageBox.question(
            self,
            "Auto-Place Lights",
            f"Add {len(new_lights)} light(s) to the highlighted positions?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Yes,
        )
        self.pending_auto_lights = None
        self.canvas.update()
        if response != QMessageBox.StandardButton.Yes:
            self._flash_status("Auto-Place Lights: cancelled.")
            return
        for candidate in new_lights:
            existing.append(candidate)
        self.apply_change("Auto-Place Lights", after)
        self._flash_status(f"Auto-Place Lights: added {len(new_lights)} light(s).")

    def open_auto_place_lights_dialog(self) -> None:
        result = AutoPlaceLightsDialog.prompt(
            self,
            self.map_data["grid_cols"],
            self.map_data["grid_rows"],
            initial=self.recent_auto_place_lights,
        )
        if result is None:
            return
        row_spacing, row_offset, col_spacing, col_offset = result
        self.auto_place_lights_on_current_level(row_spacing, row_offset, col_spacing, col_offset)

    def clear_lights_on_current_level(self) -> None:
        level_idx = self.current_level
        level = self.map_data["levels"][level_idx]
        light_count = len(level["lights"])
        if light_count == 0:
            self._flash_status("Clear Lights: this level has no lights.")
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

    def erase_lights_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        level_idx = self.current_level
        lights = self.map_data["levels"][level_idx]["lights"]
        kept = [l for l in lights if not (c0 <= l["col"] < c1 and r0 <= l["row"] < r1)]
        if len(kept) == len(lights):
            self._flash_status("Erase Lights: no lights in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][level_idx]["lights"] = kept
        self.apply_change("Erase Lights", after)

"""Map-structure actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .geometry import ramp_rect
from .normalization import level_label
from .transforms import crossing_ramps, dropped_summary, insert_level_data, remove_level_data, resize_map_data


class StructureMixin:
    # === Map structure (resize / levels / help) ===

    def resize_map(self) -> None:
        result = ResizeMapDialog.prompt(
            self, self.map_data["grid_cols"], self.map_data["grid_rows"]
        )
        if result is None:
            return
        new_cols, new_rows, anchor_x, anchor_y = result
        if new_cols == self.map_data["grid_cols"] and new_rows == self.map_data["grid_rows"]:
            return
        after = self.doc.maintain(resize_map_data(self.map_data, new_cols, new_rows, anchor_x, anchor_y))
        summary = dropped_summary(self.map_data, after)
        if summary:
            response = QMessageBox.question(
                self,
                "Resize Map",
                summary + "\n\nContinue?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.Cancel,
            )
            if response != QMessageBox.StandardButton.Yes:
                return

        self.clear_selection()
        self.apply_change("Resize Map", after)
        self.canvas.fit_map()

    def add_level(self) -> None:
        insert_at = self.current_level + 1
        crossing = crossing_ramps(self.map_data, insert_at)
        if crossing:
            self.canvas.issue_rects = [ramp_rect(ramp) for ramp in crossing]
            self.canvas.update()
            answer = QMessageBox.question(
                self, "Insert Level Through Ramps",
                f"Inserting here separates the endpoints of {len(crossing)} highlighted ramp(s). Remove those ramps and insert the level?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel, QMessageBox.StandardButton.Cancel,
            )
            self.canvas.issue_rects = []
            self.canvas.update()
            if answer != QMessageBox.StandardButton.Yes:
                return
        self.apply_change("Add Level", insert_level_data(self.map_data, insert_at, remove_crossing_ramps=True))
        self.current_level = insert_at
        self.refresh_ui()

    def rename_level(self) -> None:
        level = self.map_data["levels"][self.current_level]
        text, ok = QInputDialog.getText(self, "Rename Level", "Name:", text=level.get("name") or "")
        if not ok:
            return
        after = copy.deepcopy(self.map_data)
        after["levels"][self.current_level]["name"] = text.strip() or f"Level {self.current_level}"
        self.apply_change("Rename Level", after)

    def remove_level(self) -> None:
        if len(self.map_data["levels"]) == 1:
            QMessageBox.information(self, "Remove Level", "A map must have at least one level.")
            return
        removed = self.current_level
        after = remove_level_data(self.map_data, removed)
        level = self.map_data["levels"][removed]
        result = QMessageBox.question(
            self,
            "Remove Level",
            f"Remove {level_label(level, removed)}?\n\n" + dropped_summary(self.map_data, after),
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        if result != QMessageBox.StandardButton.Yes:
            return
        self.current_level = max(0, min(removed, len(after["levels"]) - 1))
        self.apply_change("Remove Level", after)

    def show_tool_reference(self) -> None:
        ToolReferenceDialog.open_for(self)

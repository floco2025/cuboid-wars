"""Map-structure actions for the editor window."""

from __future__ import annotations

from PySide6.QtWidgets import QMessageBox

from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .dialogs.levels import LevelsDialog
from .transforms import dropped_summary, resize_map_data


class StructureMixin:
    # === Map structure (resize / levels / help) ===

    def resize_map(self) -> None:
        result = ResizeMapDialog.prompt(self, self.map_data["grid_cols"], self.map_data["grid_rows"])
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

    def edit_levels(self) -> None:
        result = LevelsDialog.prompt(self, self.map_data, self.current_level, maintain=self.doc.maintain)
        if result is None:
            return
        after, selected = result
        if self.apply_change("Edit Levels", after):
            self.clear_selection()
        self.set_level_index(selected)

    def show_tool_reference(self) -> None:
        ToolReferenceDialog.open_for(self)

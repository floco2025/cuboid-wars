"""Map-structure actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .display import level_label
from .geometry import ramp_rect
from .normalization import empty_level
from .repairs import maintain_edit
from .transforms import GLOBAL_LISTS, LEVEL_LISTS, remap_levels, resize_map_data


def element_counts(data: dict) -> dict[str, int]:
    return {
        **{name: sum(len(level.get(name, [])) for level in data["levels"]) for name in LEVEL_LISTS},
        **{name: len(data.get(name, [])) for name in GLOBAL_LISTS},
    }


# What an edit drops, per record list, for the confirmation before it;
# empty when nothing goes.
def dropped_summary(before: dict, after: dict) -> str:
    before_counts, after_counts = element_counts(before), element_counts(after)
    parts = [
        f"{count - after_counts[name]} {name.replace('_', ' ')}"
        for name, count in before_counts.items()
        if count > after_counts[name]
    ]
    return "This will drop:\n  - " + "\n  - ".join(parts) if parts else ""


# The ramps a level inserted at `insert_at` would separate from their top.
def crossing_ramps(map_data: dict, insert_at: int) -> list[dict]:
    return [ramp for ramp in map_data["ramps"] if ramp["lower_level"] + 1 == insert_at]


def insert_level_data(map_data: dict, insert_at: int, *, remove_crossing_ramps: bool = False) -> dict:
    if crossing_ramps(map_data, insert_at) and not remove_crossing_ramps:
        raise ValueError("The inserted level separates ramp endpoints.")
    after = remap_levels(map_data, insert_at, remove=False)
    after["ramps"] = [ramp for ramp in after["ramps"] if ramp["lower_level"] + 1 != insert_at]
    after["levels"].insert(insert_at, empty_level(insert_at))
    return after


def remove_level_data(map_data: dict, removed: int) -> dict:
    if len(map_data["levels"]) <= 1:
        raise ValueError("A map needs at least one level.")
    after = remap_levels(map_data, removed, remove=True)
    after["levels"].pop(removed)
    return after


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
        after = maintain_edit(self.map_data, resize_map_data(self.map_data, new_cols, new_rows, anchor_x, anchor_y))
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

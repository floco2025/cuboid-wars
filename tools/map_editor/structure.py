"""Map-structure actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .constants import ITEMS_LIST, NESTED_MAPS_LIST, SPAWN_ZONE_LISTS
from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .display import level_label
from .geometry import ramp_rect
from .io import empty_map
from .repairs import maintain_edit
from .transforms import GLOBAL_LISTS, LEVEL_LISTS, remap_levels, resize_map_data


def element_counts(data: dict) -> dict[str, int]:
    return {
        **{name: sum(len(level.get(name, [])) for level in data["levels"]) for name in LEVEL_LISTS},
        **{name: len(data.get(name, [])) for name in GLOBAL_LISTS},
    }


def insert_level_data(map_data: dict, insert_at: int, *, remove_crossing_ramps: bool = False) -> dict:
    crossing = [ramp for ramp in map_data["ramps"] if ramp["lower_level"] + 1 == insert_at]
    if crossing and not remove_crossing_ramps:
        raise ValueError("The inserted level separates ramp endpoints.")
    after = remap_levels(map_data, insert_at, remove=False)
    after["ramps"] = [ramp for ramp in after["ramps"] if ramp["lower_level"] + 1 != insert_at]
    blank = empty_map()["levels"][0]
    blank["name"] = f"Level {insert_at}"
    after["levels"].insert(insert_at, blank)
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
        before_counts, after_counts = element_counts(self.map_data), element_counts(after)
        parts = [
            f"{count - after_counts[name]} {name.replace('_', ' ')}"
            for name, count in before_counts.items() if count > after_counts[name]
        ]
        if parts:
            response = QMessageBox.question(
                self,
                "Resize Map",
                "Resizing will drop:\n  - " + "\n  - ".join(parts) + "\n\nContinue?",
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
        crossing = [ramp for ramp in self.map_data["ramps"] if ramp["lower_level"] + 1 == insert_at]
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
        # Enumerate everything that will be dropped so the user sees the
        # blast radius before confirming. Mirrors the Resize Map dialog.
        dropped_zones = 0
        for list_name in SPAWN_ZONE_LISTS:
            dropped_zones += sum(1 for zone in self.map_data[list_name] if zone["level"] == removed)
        dropped_items = sum(1 for item in self.map_data.get(ITEMS_LIST, []) if item["level"] == removed)
        dropped_plates = sum(1 for plate in self.map_data.get("pressure_plates", []) if plate["level"] == removed)
        dropped_ramps = 0
        for ramp in self.map_data["ramps"]:
            lower = ramp["lower_level"]
            if removed in (lower, lower + 1):
                dropped_ramps += 1
        dropped_ladders = sum(
            1
            for ladder in self.map_data.get("ladders", [])
            if ladder["lower_level"] <= removed <= ladder["lower_level"] + ladder["levels"]
        )
        dropped_nested = sum(
            1
            for entry in self.map_data.get(NESTED_MAPS_LIST, [])
            if min(entry["level"], entry["to_level"]) <= removed <= max(entry["level"], entry["to_level"])
        )
        level = self.map_data["levels"][removed]
        floor_count = len(level["floors"]) + len(level["inaccessible_floors"])
        wall_count = len(level["walls"])
        light_count = len(level["lights"])
        barrier_count = len(level.get("barriers", []))
        bridge_count = len(level.get("light_bridges", []))
        parts = [f"all geometry on {level_label(level, removed)}"]
        details = []
        if floor_count:
            details.append(f"{floor_count} floor cell(s)")
        if wall_count:
            details.append(f"{wall_count} wall(s)")
        if light_count:
            details.append(f"{light_count} light(s)")
        if barrier_count:
            details.append(f"{barrier_count} barrier(s)")
        if bridge_count:
            details.append(f"{bridge_count} light bridge(s)")
        if dropped_zones:
            parts.append(f"{dropped_zones} spawn zone(s) on this level")
        if dropped_items:
            parts.append(f"{dropped_items} item(s) on this level")
        if dropped_plates:
            parts.append(f"{dropped_plates} pressure plate(s) on this level")
        if dropped_ramps:
            parts.append(f"{dropped_ramps} ramp(s) that span this level")
        if dropped_ladders:
            parts.append(f"{dropped_ladders} ladder(s) that span this level")
        if dropped_nested:
            parts.append(f"{dropped_nested} nested map(s) whose ends touch this level")
        body = f"Remove {level_label(level, removed)}?\n\nThis will drop:"
        body += "\n  - " + "\n  - ".join(parts)
        if details:
            body += "\n\n(geometry: " + ", ".join(details) + ")"
        result = QMessageBox.question(
            self,
            "Remove Level",
            body,
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        if result != QMessageBox.StandardButton.Yes:
            return
        after = remove_level_data(self.map_data, removed)
        self.current_level = max(0, min(removed, len(after["levels"]) - 1))
        self.apply_change("Remove Level", after)

    def show_tool_reference(self) -> None:
        ToolReferenceDialog.open_for(self)

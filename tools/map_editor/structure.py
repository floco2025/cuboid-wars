"""Map-structure actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .constants import ITEMS_LIST, NESTED_MAPS_LIST, SPAWN_ZONE_LISTS
from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .display import level_label
from .normalization import canonicalize_map, resize_map_data
from .regions import GLOBAL_LISTS, LEVEL_LISTS


def element_counts(data: dict) -> dict[str, int]:
    return {
        **{name: sum(len(level.get(name, [])) for level in data["levels"]) for name in LEVEL_LISTS},
        **{name: len(data.get(name, [])) for name in GLOBAL_LISTS},
    }


def insert_level_data(map_data: dict, insert_at: int) -> dict:
    """A copy of `map_data` with an empty level at `insert_at`; everything
    on or above it moves up one storey."""
    after = copy.deepcopy(map_data)
    after["levels"].insert(
        insert_at,
        {
            "name": f"Level {insert_at}",
            "floors": [],
            "inaccessible_floors": [],
            "grass": [],
            "walls": [],
            "barriers": [],
            "light_bridges": [],
            "lights": [],
        },
    )
    for list_name in SPAWN_ZONE_LISTS:
        for zone in after[list_name]:
            if zone["level"] >= insert_at:
                zone["level"] += 1
    for item in after.get(ITEMS_LIST, []):
        if item["level"] >= insert_at:
            item["level"] += 1
    for plate in after.get("pressure_plates", []):
        if plate["level"] >= insert_at:
            plate["level"] += 1
    for ramp in after["ramps"]:
        if ramp["lower_level"] >= insert_at:
            ramp["lower_level"] += 1
    for ladder in after.get("ladders", []):
        if ladder["lower_level"] >= insert_at:
            ladder["lower_level"] += 1
        elif ladder["lower_level"] + ladder["levels"] >= insert_at:
            # The insertion lands inside the span: stretch so both
            # endpoints keep their storeys.
            ladder["levels"] += 1
    # A nested map's own storeys live in its file; only its ends move, each
    # keeping its storey, so a lift the insertion lands inside stretches by
    # itself.
    for entry in after.get(NESTED_MAPS_LIST, []):
        for key in ("level", "to_level"):
            if entry[key] >= insert_at:
                entry[key] += 1
    return after


def remove_level_data(map_data: dict, removed: int) -> dict:
    """A copy of `map_data` without level `removed`: everything on it goes,
    everything spanning it goes, everything above it moves down one storey."""
    after = copy.deepcopy(map_data)
    after["levels"].pop(removed)
    for list_name in (*SPAWN_ZONE_LISTS, ITEMS_LIST, "pressure_plates"):
        adjusted_entries = []
        for entry in after.get(list_name, []):
            if entry["level"] == removed:
                continue
            if entry["level"] > removed:
                entry["level"] -= 1
            adjusted_entries.append(entry)
        after[list_name] = adjusted_entries
    adjusted = []
    for ramp in after["ramps"]:
        lower = ramp["lower_level"]
        upper = lower + 1
        if removed in (lower, upper):
            continue
        if lower > removed:
            ramp["lower_level"] = lower - 1
        adjusted.append(ramp)
    after["ramps"] = adjusted
    adjusted_ladders = []
    for ladder in after.get("ladders", []):
        lower = ladder["lower_level"]
        if lower <= removed <= lower + ladder["levels"]:
            continue
        if lower > removed:
            ladder["lower_level"] = lower - 1
        adjusted_ladders.append(ladder)
    after["ladders"] = adjusted_ladders
    adjusted_nested = []
    for entry in after.get(NESTED_MAPS_LIST, []):
        if min(entry["level"], entry["to_level"]) <= removed <= max(entry["level"], entry["to_level"]):
            continue
        for key in ("level", "to_level"):
            if entry[key] > removed:
                entry[key] -= 1
        adjusted_nested.append(entry)
    after[NESTED_MAPS_LIST] = adjusted_nested
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
        after = canonicalize_map(resize_map_data(self.map_data, new_cols, new_rows, anchor_x, anchor_y))
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
        self.resize_to_map()

    def add_level(self) -> None:
        insert_at = self.current_level + 1
        self.apply_change("Add Level", insert_level_data(self.map_data, insert_at))
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

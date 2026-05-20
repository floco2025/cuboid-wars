"""Map-structure actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog, QMessageBox

from .constants import SPAWN_ZONE_LISTS
from .dialogs import ResizeMapDialog, ToolReferenceDialog
from .geometry import level_label, resize_map_data


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
        after = resize_map_data(self.map_data, new_cols, new_rows, anchor_x, anchor_y)

        before_floors = sum(len(l["floors"]) for l in self.map_data["levels"])
        before_inacc = sum(len(l["inaccessible_floors"]) for l in self.map_data["levels"])
        before_walls = sum(len(l["walls"]) for l in self.map_data["levels"])
        before_zones = len(self.map_data["actor_spawn_zones"]) + len(self.map_data["player_spawn_zones"])
        before_ramps = len(self.map_data["ramps"])
        after_floors = sum(len(l["floors"]) for l in after["levels"])
        after_inacc = sum(len(l["inaccessible_floors"]) for l in after["levels"])
        after_walls = sum(len(l["walls"]) for l in after["levels"])
        after_zones = len(after["actor_spawn_zones"]) + len(after["player_spawn_zones"])
        after_ramps = len(after["ramps"])
        dropped_floors = before_floors - after_floors
        dropped_inacc = before_inacc - after_inacc
        dropped_walls = before_walls - after_walls
        dropped_zones = before_zones - after_zones
        dropped_ramps = before_ramps - after_ramps
        if any(n > 0 for n in (dropped_floors, dropped_inacc, dropped_walls, dropped_zones, dropped_ramps)):
            parts = []
            if dropped_floors:
                parts.append(f"{dropped_floors} floor cell(s)")
            if dropped_inacc:
                parts.append(f"{dropped_inacc} inaccessible-floor cell(s)")
            if dropped_walls:
                parts.append(f"{dropped_walls} wall(s)")
            if dropped_zones:
                parts.append(f"{dropped_zones} spawn zone(s)")
            if dropped_ramps:
                parts.append(f"{dropped_ramps} ramp(s)")
            response = QMessageBox.question(
                self,
                "Resize Map",
                "Resizing will drop:\n  - " + "\n  - ".join(parts) + "\n\nContinue?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.Cancel,
            )
            if response != QMessageBox.StandardButton.Yes:
                return

        self.apply_change("Resize Map", after)
        self.resize_to_map()

    def add_level(self) -> None:
        after = copy.deepcopy(self.map_data)
        insert_at = self.current_level + 1
        after["levels"].insert(
            insert_at,
            {
                "name": f"Level {insert_at}",
                "floors": [],
                "inaccessible_floors": [],
                "walls": [],
                "barriers": [],
                "lights": [],
            },
        )
        for list_name in SPAWN_ZONE_LISTS:
            for zone in after[list_name]:
                if zone["level"] >= insert_at:
                    zone["level"] += 1
        for ramp in after["ramps"]:
            if ramp["lower_level"] >= insert_at:
                ramp["lower_level"] += 1
        self.apply_change("Add Level", after)
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
        dropped_ramps = 0
        for ramp in self.map_data["ramps"]:
            lower = ramp["lower_level"]
            if removed in (lower, lower + 1):
                dropped_ramps += 1
        level = self.map_data["levels"][removed]
        floor_count = len(level["floors"]) + len(level["inaccessible_floors"])
        wall_count = len(level["walls"])
        light_count = len(level["lights"])
        barrier_count = len(level.get("barriers", []))
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
        if dropped_zones:
            parts.append(f"{dropped_zones} spawn zone(s) on this level")
        if dropped_ramps:
            parts.append(f"{dropped_ramps} ramp(s) that span this level")
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
        after = copy.deepcopy(self.map_data)
        after["levels"].pop(removed)
        for list_name in SPAWN_ZONE_LISTS:
            adjusted_zones = []
            for zone in after[list_name]:
                if zone["level"] == removed:
                    continue
                if zone["level"] > removed:
                    zone["level"] -= 1
                adjusted_zones.append(zone)
            after[list_name] = adjusted_zones
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
        self.current_level = max(0, min(removed, len(after["levels"]) - 1))
        self.apply_change("Remove Level", after)

    def show_tool_reference(self) -> None:
        ToolReferenceDialog.open_for(self)

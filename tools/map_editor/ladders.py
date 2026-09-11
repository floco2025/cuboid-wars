"""Ladder placement actions for the editor window."""

from __future__ import annotations

import copy

from PySide6.QtWidgets import QInputDialog

from .constants import LADDER_SIDES
from .editing import update_records
from .geometry import cell_side_from_click, ladder_anchor_from_click, wall_endpoints_for_cell_side
from .normalization import ladder_edge_key, ladder_key, ladder_spans_level, ladders_overlap


class LaddersMixin:
    # === Ladders ===

    def edit_ladder_at(self, key: tuple) -> None:
        ladder = next((ladder for ladder in self.map_data["ladders"] if ladder_key(ladder) == key), None)
        if ladder is None:
            return
        max_levels = len(self.map_data["levels"]) - 1 - ladder["lower_level"]
        if max_levels < 1:
            self.notify("A ladder needs a level above its base to climb to.")
            return
        levels, accepted = QInputDialog.getInt(
            self, "Edit Ladder", "Storeys:", min(max_levels, max(1, ladder["levels"])), 1, max_levels,
        )
        if not accepted or levels == ladder["levels"]:
            return
        candidate = {**ladder, "levels": levels}
        if any(ladder_key(other) != key and ladders_overlap(candidate, other) for other in self.map_data["ladders"]):
            self.notify("A ladder already spans part of that edge.")
            return
        after = update_records(self.map_data, "ladders", lambda ladder: ladder_key(ladder) == key, {"levels": levels})
        self.apply_change("Edit Ladder", after)

    def toggle_ladder_at(self, pos) -> None:
        px = pos.x()
        py = pos.y()
        cols = self.map_data["grid_cols"]
        rows = self.map_data["grid_rows"]
        col = int(px)
        row = int(py)
        if not (0 <= col < cols and 0 <= row < rows):
            return
        # The clicked cell is where the ladder physically stands; the stored
        # anchor is the cell across the edge (its floors are the landings).
        side = cell_side_from_click(col, row, px, py)
        anchor_col, anchor_row, anchor_side = ladder_anchor_from_click(col, row, side)
        level_idx = self.current_level

        # Clicking an edge that already holds a ladder touching this level
        # removes it (toggle, like lights). Matched by the undirected edge so
        # a click from either side of the line toggles the same ladder.
        edge = wall_endpoints_for_cell_side(col, row, side)
        existing = next(
            (
                l for l in self.map_data.get("ladders", [])
                if l["side"] in LADDER_SIDES and ladder_edge_key(l) == edge and ladder_spans_level(l, level_idx)
            ),
            None,
        )
        if existing is not None:
            after = copy.deepcopy(self.map_data)
            after["ladders"] = [l for l in after["ladders"] if ladder_key(l) != ladder_key(existing)]
            self.apply_change("Remove Ladder", after)
            return

        if not (0 <= anchor_col < cols and 0 <= anchor_row < rows):
            self.notify("No cell across that edge to climb to.")
            return
        max_levels = len(self.map_data["levels"]) - 1 - level_idx
        if max_levels < 1:
            self.notify("A ladder needs a level above this one to climb to.")
            return
        levels = min(max_levels, max(1, self.recent_ladder_levels))
        self.recent_ladder_levels = levels

        new_ladder = {
            "lower_level": level_idx,
            "col": anchor_col,
            "row": anchor_row,
            "side": anchor_side,
            "levels": levels,
        }
        if any(ladders_overlap(new_ladder, l) for l in self.map_data.get("ladders", [])):
            self.notify(f"A ladder already spans that edge ({side} side of [{col}, {row}]).")
            return
        after = copy.deepcopy(self.map_data)
        after.setdefault("ladders", []).append(new_ladder)
        self.apply_change("Add Ladder", after)

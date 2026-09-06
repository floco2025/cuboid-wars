"""Ladder placement actions for the editor window."""

from __future__ import annotations

import copy

from .constants import LADDER_SIDES
from .erasing import ladders_outside
from .geometry import (
    cell_side_from_click,
    ladder_anchor_from_click,
    rect_from_cells,
    wall_endpoints_for_cell_side,
)
from .normalization import ladder_key, ladder_spans_level


class LaddersMixin:
    # === Ladders ===

    def toggle_ladder_at(self, pos, cell_size: float) -> None:
        px = pos.x() / cell_size
        py = pos.y() / cell_size
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
                if l["side"] in LADDER_SIDES and wall_endpoints_for_cell_side(l["col"], l["row"], l["side"]) == edge
                and ladder_spans_level(l, level_idx)
            ),
            None,
        )
        if existing is not None:
            after = copy.deepcopy(self.map_data)
            after["ladders"] = [l for l in after["ladders"] if ladder_key(l) != ladder_key(existing)]
            self.apply_change("Remove Ladder", after)
            return

        if not (0 <= anchor_col < cols and 0 <= anchor_row < rows):
            self._flash_status("No cell across that edge to climb to.")
            return
        max_levels = len(self.map_data["levels"]) - 1 - level_idx
        if max_levels < 1:
            self._flash_status("A ladder needs a level above this one to climb to.")
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
        overlapping = any(
            l["side"] in LADDER_SIDES and wall_endpoints_for_cell_side(l["col"], l["row"], l["side"]) == edge
            and l["lower_level"] < level_idx + levels
            and level_idx < l["lower_level"] + l["levels"]
            for l in self.map_data.get("ladders", [])
        )
        if overlapping:
            self._flash_status(f"A ladder already spans that edge ({side} side of [{col}, {row}]).")
            return
        after = copy.deepcopy(self.map_data)
        after.setdefault("ladders", []).append(new_ladder)
        self.apply_change("Add Ladder", after)

    def erase_ladders_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        rect = rect_from_cells(start, end)
        level_idx = self.current_level
        ladders = self.map_data.get("ladders", [])
        kept = ladders_outside(ladders, level_idx, rect)
        if len(kept) == len(ladders):
            self._flash_status("Erase Ladders: no ladders in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after["ladders"] = kept
        self.apply_change("Erase Ladders", after)

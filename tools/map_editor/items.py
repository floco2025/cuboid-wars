"""Placed-item actions for the editor window."""

from __future__ import annotations

import copy

from .constants import ITEM_KEY_TYPE, ITEMS_LIST
from .dialogs import ItemTypeDialog
from .geometry import ramp_cells, rect_from_cells


class ItemsMixin:
    # === Items ===

    def _item_cell_error(self, col: int, row: int) -> str | None:
        level = self.map_data["levels"][self.current_level]
        if (col, row) not in {(f["col"], f["row"]) for f in level["floors"]}:
            return f"cell [{col}, {row}] has no regular floor."
        # The server marks `has_ramp` only on the lower level's footprint
        # cells, so the upper level of a ramp stays placeable.
        for ramp in self.map_data["ramps"]:
            if ramp["lower_level"] == self.current_level and (col, row) in ramp_cells(ramp):
                return f"cell [{col}, {row}] is inside a ramp footprint."
        return None

    def prompt_and_add_item(self, col: int, row: int) -> None:
        result = ItemTypeDialog.prompt(
            self, "Place Item", self.barrier_kinds, self.recent_item_type, self.recent_item_key_kind
        )
        if result is None:
            return
        item_type, kind = result
        self.recent_item_type = item_type
        if kind is not None:
            self.recent_item_key_kind = kind
        self.add_item(col, row, item_type, kind)

    def add_item(self, col: int, row: int, item_type: str, kind: str | None) -> None:
        error = self._item_cell_error(col, row)
        if error is not None:
            self._flash_status(f"Item not placed: {error}")
            return
        if item_type == ITEM_KEY_TYPE and kind not in self.barrier_kinds:
            self._flash_status(f"Unknown key kind {kind!r}")
            return
        after = copy.deepcopy(self.map_data)
        items = after.setdefault(ITEMS_LIST, [])
        # Same-cell placement replaces the existing item.
        items[:] = [
            i for i in items if not (i["level"] == self.current_level and i["col"] == col and i["row"] == row)
        ]
        new_item = {"level": self.current_level, "col": col, "row": row, "type": item_type}
        if item_type == ITEM_KEY_TYPE:
            new_item["kind"] = kind
        items.append(new_item)
        label = f"Place Item ({item_type} {kind})" if kind else f"Place Item ({item_type})"
        self.apply_change(label, after)

    def remove_item_at(self, col: int, row: int) -> bool:
        """Remove any item at (current_level, col, row). Returns True if an
        item was removed."""
        after = copy.deepcopy(self.map_data)
        items = after.get(ITEMS_LIST, [])
        keep = [i for i in items if not (i["level"] == self.current_level and i["col"] == col and i["row"] == row)]
        if len(keep) == len(items):
            return False
        after[ITEMS_LIST] = keep
        self.apply_change("Remove Item", after)
        return True

    def erase_items_rect(self, start: tuple[int, int], end: tuple[int, int]) -> None:
        c0, r0, c1, r1 = rect_from_cells(start, end)
        items = self.map_data.get(ITEMS_LIST, [])
        kept = [
            i
            for i in items
            if not (i["level"] == self.current_level and c0 <= i["col"] < c1 and r0 <= i["row"] < r1)
        ]
        if len(kept) == len(items):
            self._flash_status("Erase Items: no items in selection.")
            return
        after = copy.deepcopy(self.map_data)
        after[ITEMS_LIST] = kept
        self.apply_change("Erase Items", after)

"""Placed-item actions for the editor window."""

from __future__ import annotations

import copy

from .constants import ITEM_KEY_TYPE, ITEMS_LIST, ITEM_TYPES
from .dialogs import ItemTypeDialog
from .geometry import ramp_cells


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

    def item_at(self, col: int, row: int) -> dict | None:
        return next(
            (
                item
                for item in self.map_data.get(ITEMS_LIST, [])
                if item["level"] == self.current_level and (item["col"], item["row"]) == (col, row)
            ),
            None,
        )

    def prompt_and_add_item(self, col: int, row: int) -> None:
        if self.item_at(col, row) is not None:
            self.notify(f"Item not placed: cell [{col}, {row}] already holds one; right-click it to edit or erase.")
            return
        if self.recent_item_type in ITEM_TYPES:
            if self.recent_item_type != ITEM_KEY_TYPE or self.recent_item_key_kind in self.barrier_kinds:
                self.add_item(col, row, self.recent_item_type, self.recent_item_key_kind)
                return
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

    def edit_item_at(self, col: int, row: int) -> None:
        item = self.item_at(col, row)
        if item is None:
            return
        result = ItemTypeDialog.prompt(self, "Edit Item", self.barrier_kinds, item["type"], item.get("kind"))
        if result is None:
            return
        item_type, kind = result
        self.add_item(col, row, item_type, kind, label="Edit Item")

    def add_item(self, col: int, row: int, item_type: str, kind: str | None, label: str | None = None) -> None:
        error = self._item_cell_error(col, row)
        if error is not None:
            self.notify(f"Item not placed: {error}")
            return
        if item_type == ITEM_KEY_TYPE and kind not in self.barrier_kinds:
            self.notify(f"Unknown key kind {kind!r}")
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
        if label is None:
            label = f"Place Item ({item_type} {kind})" if kind else f"Place Item ({item_type})"
        self.apply_change(label, after)

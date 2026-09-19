"""Placed-item actions for the editor window."""

from __future__ import annotations

import copy

from .constants import ITEM_KEY_TYPE, ITEMS_LIST
from .dialogs import ItemTypeDialog
from .normalization import item_cell_error


class ItemsMixin:
    # === Items ===

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
        if self.recent_item_type in self.pickup_types:
            if self.recent_item_type != ITEM_KEY_TYPE or self.recent_item_key_field in self.fields:
                self.add_item(col, row, self.recent_item_type, self.recent_item_key_field)
                return
        result = ItemTypeDialog.prompt(
            self,
            "Place Item",
            self.fields,
            self.recent_item_type,
            self.recent_item_key_field,
            self.field_colors,
            item_types=self.pickup_types,
        )
        if result is None:
            return
        item_type, field = result
        self.recent_item_type = item_type
        if field is not None:
            self.recent_item_key_field = field
        self.add_item(col, row, item_type, field)

    def add_item(self, col: int, row: int, item_type: str, field: str | None, label: str | None = None) -> None:
        if item_type not in self.pickup_types:
            self.notify(f"Item not placed: {item_type} is unavailable as a pickup.")
            return
        error = item_cell_error(self.map_data, self.current_level, col, row)
        if error is not None:
            self.notify(f"Item not placed: cell {error}.")
            return
        if item_type == ITEM_KEY_TYPE and field not in self.fields:
            self.notify(f"Unknown key field {field!r}")
            return
        after = copy.deepcopy(self.map_data)
        items = after.setdefault(ITEMS_LIST, [])
        # Same-cell placement replaces the existing item.
        items[:] = [i for i in items if not (i["level"] == self.current_level and i["col"] == col and i["row"] == row)]
        new_item = {"level": self.current_level, "col": col, "row": row, "type": item_type}
        if item_type == ITEM_KEY_TYPE:
            new_item["field"] = field
        items.append(new_item)
        if label is None:
            label = f"Place Item ({item_type} {field})" if field else f"Place Item ({item_type})"
        self.apply_change(label, after)

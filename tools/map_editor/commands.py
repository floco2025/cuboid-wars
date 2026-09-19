"""Undo/redo commands for map edits."""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtGui import QUndoCommand

if TYPE_CHECKING:
    from .document import MapDocument


class SetMapCommand(QUndoCommand):
    MERGEABLE_ID = 1

    # Consecutive commands sharing a `merge_key` undo as one step.
    def __init__(
        self,
        document: "MapDocument",
        text: str,
        before: dict,
        after: dict,
        active_map: str | None,
        merge_key: object | None = None,
    ):
        super().__init__(text)
        self.document = document
        self.before = before
        self.after = after
        self.before_map = document.active_map
        self.after_map = active_map
        self.merge_key = merge_key

    def id(self) -> int:
        return -1 if self.merge_key is None else self.MERGEABLE_ID

    def mergeWith(self, other: QUndoCommand) -> bool:
        if not isinstance(other, SetMapCommand) or other.merge_key is not self.merge_key:
            return False
        self.after = other.after
        self.after_map = other.after_map
        # Edits that cancel out leave no step behind.
        if self.after == self.before and self.after_map == self.before_map:
            self.setObsolete(True)
        return True

    def undo(self) -> None:
        self.document.set_data(self.before, mark_dirty=True, active_map=self.before_map)

    def redo(self) -> None:
        self.document.set_data(self.after, mark_dirty=True, active_map=self.after_map)

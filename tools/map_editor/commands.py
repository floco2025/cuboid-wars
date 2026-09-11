"""Undo/redo commands for map edits."""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtGui import QUndoCommand

if TYPE_CHECKING:
    from .document import MapDocument


class SetMapCommand(QUndoCommand):
    def __init__(self, document: "MapDocument", text: str, before: dict, after: dict, active_map: str | None):
        super().__init__(text)
        self.document = document
        self.before = before
        self.after = after
        self.before_map = document.active_map
        self.after_map = active_map

    def undo(self) -> None:
        self.document.set_data(self.before, mark_dirty=True, active_map=self.before_map)

    def redo(self) -> None:
        self.document.set_data(self.after, mark_dirty=True, active_map=self.after_map)

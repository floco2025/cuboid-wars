"""Undo/redo commands for map edits."""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtGui import QUndoCommand

if TYPE_CHECKING:
    from .document import MapDocument

class SetMapCommand(QUndoCommand):
    def __init__(self, document: "MapDocument", text: str, before: dict, after: dict):
        super().__init__(text)
        self.document = document
        self.before = before
        self.after = after

    def undo(self) -> None:
        self.document.set_data(self.before, mark_dirty=True)

    def redo(self) -> None:
        self.document.set_data(self.after, mark_dirty=True)

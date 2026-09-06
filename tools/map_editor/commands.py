"""Undo/redo commands for map edits."""

from __future__ import annotations

from PySide6.QtGui import QUndoCommand

class SetMapCommand(QUndoCommand):
    def __init__(self, window: "EditorWindow", text: str, before: dict, after: dict):
        super().__init__(text)
        self.window = window
        self.before = before
        self.after = after

    def undo(self) -> None:
        self.window.set_map(self.before, mark_dirty=True)

    def redo(self) -> None:
        self.window.set_map(self.after, mark_dirty=True)

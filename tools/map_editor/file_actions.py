"""File actions for the editor window."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import QFileDialog, QMessageBox

from .constants import DEFAULT_MAP
from .io import read_map, write_map
from .validation import validate_map


class FileActionsMixin:
    # === File I/O ===

    def open_file(self) -> None:
        if not self.confirm_discard_changes():
            return
        path, _ = QFileDialog.getOpenFileName(self, "Open Map", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        try:
            self.map_data = read_map(Path(path))
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        self.path = Path(path)
        self.current_level = 0
        self.dirty = False
        self.undo_stack.clear()
        self.refresh_ui()

    def save(self) -> None:
        if self.path is None:
            self.save_as()
            return
        errors = validate_map(self.map_data)
        if errors:
            QMessageBox.warning(
                self,
                "Cannot Save",
                "Fix structural issues before saving:\n\n" + "\n".join(errors[:12]),
            )
            return
        try:
            write_map(self.path, self.map_data)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return
        self.dirty = False
        self.refresh_ui()

    def save_as(self) -> None:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        self.path = Path(path)
        self.save()

    def confirm_discard_changes(self) -> bool:
        if not self.dirty:
            return True
        result = QMessageBox.question(
            self,
            "Unsaved Changes",
            "Discard unsaved changes?",
            QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Cancel,
        )
        return result == QMessageBox.StandardButton.Discard

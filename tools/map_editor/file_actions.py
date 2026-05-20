"""File actions for the editor window."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import QFileDialog, QMessageBox

from .constants import DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS, DEFAULT_MAP
from .dialogs import ResizeMapDialog
from .io import empty_map, read_map, write_map
from .validation import validate_map


class FileActionsMixin:
    # === File I/O ===

    def new_file(self) -> None:
        # Discard prompt first — same protection the file-open path uses.
        if not self.confirm_discard_changes():
            return
        # Reuse ResizeMapDialog with the default grid as the seed values. The
        # anchor pads are irrelevant for an empty map; we ignore them.
        result = ResizeMapDialog.prompt(self, DEFAULT_GRID_COLS, DEFAULT_GRID_ROWS)
        if result is None:
            return
        new_cols, new_rows, _, _ = result
        self.map_data = empty_map(new_cols, new_rows)
        self.path = None
        self.path_mtime = None
        self.current_level = 0
        # Fresh map = unsaved by definition; dropping the asterisk would be
        # misleading until the user picks a destination.
        self.dirty = True
        self.undo_stack.clear()
        self.refresh_ui()

    def open_file(self) -> None:
        if not self.confirm_discard_changes():
            return
        path, _ = QFileDialog.getOpenFileName(self, "Open Map", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        self.load_path(Path(path))

    def load_path(self, path: Path) -> None:
        try:
            loaded = read_map(path)
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        # Surface structural issues at load time instead of waiting for the
        # user to discover them on save. We still let them load (so they
        # can edit and fix), but the modal makes the problems visible.
        errors = validate_map(loaded)
        if errors:
            QMessageBox.warning(
                self,
                "Map Has Structural Issues",
                "This map has issues that will block saving until fixed:\n\n"
                + "\n".join(errors[:12])
                + ("\n…" if len(errors) > 12 else ""),
            )
        self.map_data = loaded
        self.path = path
        self.path_mtime = path.stat().st_mtime
        self.current_level = 0
        self.dirty = False
        self.undo_stack.clear()
        self._record_recent_path(path)
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
        # External-modification check: if the file's mtime changed under us,
        # ask before clobbering. Skip when we have no recorded baseline
        # (fresh map / Save As to a new path).
        if self.path.exists() and self.path_mtime is not None:
            current_mtime = self.path.stat().st_mtime
            if current_mtime > self.path_mtime + 1e-3:
                result = QMessageBox.question(
                    self,
                    "File Changed Externally",
                    f"{self.path} was modified outside the editor since it was opened. "
                    "Overwrite with your in-editor version?",
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                    QMessageBox.StandardButton.Cancel,
                )
                if result != QMessageBox.StandardButton.Yes:
                    return
        try:
            write_map(self.path, self.map_data)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return
        self.path_mtime = self.path.stat().st_mtime
        self.dirty = False
        self._clear_autosave()
        self._record_recent_path(self.path)
        self.refresh_ui()

    def save_as(self) -> None:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or DEFAULT_MAP), "JSON files (*.json)")
        if not path:
            return
        self.path = Path(path)
        # No baseline mtime for the new destination — we never read it, so any
        # existing file at this path is something the user chose to overwrite.
        self.path_mtime = None
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
        if result == QMessageBox.StandardButton.Discard:
            # User chose to discard — the autosave is a persisted copy of the
            # changes they just rejected, so it must go too. Otherwise next
            # launch would offer to "recover" the discarded work.
            self._clear_autosave()
            return True
        return False

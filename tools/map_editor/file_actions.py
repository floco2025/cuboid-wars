"""File actions for the editor window."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import QFileDialog, QMessageBox

from .constants import (
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    MAPS_DIR,
    load_map_barrier_kinds,
    load_map_bridge_kinds,
)
from .dialogs import ResizeMapDialog
from .io import empty_map, read_map
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
        self.doc.replace_with_new(empty_map(new_cols, new_rows))
        self.barrier_kind_colors = {}
        self.bridge_kind_colors = {}
        self.current_level = 0
        self.refresh_ui()

    def open_file(self) -> None:
        if not self.confirm_discard_changes():
            return
        path, _ = QFileDialog.getOpenFileName(self, "Open Map", str(self.path or MAPS_DIR), "JSON files (*.json)")
        if not path:
            return
        self.load_path(Path(path))

    def load_path(self, path: Path) -> None:
        try:
            loaded = read_map(path)
            barrier_kinds = load_map_barrier_kinds(path.stem)
            bridge_kinds = load_map_bridge_kinds(path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        # Surface structural issues at load time instead of waiting for the
        # user to discover them on save. We still let them load (so they
        # can edit and fix), but the modal makes the problems visible.
        errors = validate_map(loaded, list(barrier_kinds), list(bridge_kinds))
        if errors:
            QMessageBox.warning(
                self,
                "Map Has Structural Issues",
                "This map has issues that will block saving until fixed:\n\n"
                + "\n".join(errors[:12])
                + ("\n…" if len(errors) > 12 else ""),
            )
        self.doc.load(path)
        self.barrier_kind_colors = barrier_kinds
        self.bridge_kind_colors = bridge_kinds
        self.current_level = 0
        self._record_recent_path(path)
        self.refresh_ui()

    def save(self) -> None:
        if self.path is None:
            self.save_as()
            return
        errors = validate_map(self.map_data, self.barrier_kinds, self.bridge_kinds)
        if errors:
            QMessageBox.warning(
                self,
                "Cannot Save",
                "Fix structural issues before saving:\n\n" + "\n".join(errors[:12]),
            )
            return
        # External-modification check: if the file's mtime changed under us,
        # ask before clobbering.
        if self.doc.externally_modified():
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
            self.doc.write()
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return
        self._record_recent_path(self.path)
        self.refresh_ui()

    def save_as(self) -> None:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or MAPS_DIR), "JSON files (*.json)")
        if not path:
            return
        new_path = Path(path)
        try:
            barrier_kinds = load_map_barrier_kinds(new_path.stem)
            bridge_kinds = load_map_bridge_kinds(new_path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return
        self.path = new_path
        self.barrier_kind_colors = barrier_kinds
        self.bridge_kind_colors = bridge_kinds
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
        return result == QMessageBox.StandardButton.Discard

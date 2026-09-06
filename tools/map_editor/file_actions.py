"""File actions for the editor window."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QFileDialog, QMessageBox

from .constants import (
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    MAPS_DIR,
    DEFAULT_WALL_WIDTH_CELLS,
    load_map_barrier_kinds,
    load_map_bridge_kinds,
    load_map_wall_width_cells,
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
        self.clear_selection()
        self.barrier_kind_colors = {}
        self.bridge_kind_colors = {}
        self.wall_width_cells = DEFAULT_WALL_WIDTH_CELLS
        self.forget_nested_map_shapes()
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
            loaded_mtime = path.stat().st_mtime
            loaded = read_map(path)
            barrier_kinds = load_map_barrier_kinds(path.stem)
            bridge_kinds = load_map_bridge_kinds(path.stem)
            wall_width_cells = load_map_wall_width_cells(path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        # Surface structural issues at load time instead of waiting for the
        # user to discover them on save. We still let them load (so they
        # can edit and fix), but the modal makes the problems visible.
        self.forget_nested_map_shapes()
        errors = validate_map(
            loaded, list(barrier_kinds), list(bridge_kinds),
            map_name=path.stem, nested_lookup=self.nested_map_shape,
        )
        if errors:
            QMessageBox.warning(
                self,
                "Map Has Structural Issues",
                "This map has issues that will block saving until fixed:\n\n"
                + "\n".join(errors[:12])
                + ("\n…" if len(errors) > 12 else ""),
            )
        try:
            self.doc.load(path, loaded, loaded_mtime)
        except OSError as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        self.clear_selection()
        self.barrier_kind_colors = barrier_kinds
        self.bridge_kind_colors = bridge_kinds
        self.wall_width_cells = wall_width_cells
        self.forget_nested_map_shapes()
        self.current_level = 0
        self._record_recent_path(path)
        self.refresh_ui()
        QTimer.singleShot(0, self.maybe_recover_autosave)

    def save(self) -> bool:
        if self.path is None:
            return self.save_as()
        return self._save_to(self.path, self.barrier_kind_colors, self.bridge_kind_colors, self.wall_width_cells)

    def _save_to(self, path: Path, barrier_kinds: dict, bridge_kinds: dict, wall_width_cells: float) -> bool:
        self.forget_nested_map_shapes()
        errors = validate_map(
            self.map_data, list(barrier_kinds), list(bridge_kinds),
            map_name=path.stem, nested_lookup=self.nested_map_shape,
        )
        if errors:
            QMessageBox.warning(
                self,
                "Cannot Save",
                "Fix structural issues before saving:\n\n" + "\n".join(errors[:12]),
            )
            return False
        # External-modification check: if the file's mtime changed under us,
        # ask before clobbering.
        if path == self.path and self.doc.externally_modified():
            result = QMessageBox.question(
                self,
                "File Changed Externally",
                f"{self.path} was modified outside the editor since it was opened. "
                "Overwrite with your in-editor version?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.Cancel,
            )
            if result != QMessageBox.StandardButton.Yes:
                return False
        try:
            self.doc.write(path)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return False
        self.barrier_kind_colors = barrier_kinds
        self.bridge_kind_colors = bridge_kinds
        self.wall_width_cells = wall_width_cells
        self._record_recent_path(self.path)
        self.forget_nested_map_shapes()
        self.refresh_ui()
        return True

    def save_as(self) -> bool:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or MAPS_DIR), "JSON files (*.json)")
        if not path:
            return False
        new_path = Path(path)
        try:
            barrier_kinds = load_map_barrier_kinds(new_path.stem)
            bridge_kinds = load_map_bridge_kinds(new_path.stem)
            wall_width_cells = load_map_wall_width_cells(new_path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return False
        return self._save_to(new_path, barrier_kinds, bridge_kinds, wall_width_cells)

    def confirm_discard_changes(self) -> bool:
        if not self.dirty:
            return True
        result = QMessageBox.question(
            self,
            "Unsaved Changes",
            "Save changes before continuing?",
            QMessageBox.StandardButton.Save | QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )
        if result == QMessageBox.StandardButton.Save:
            return self.save()
        return result == QMessageBox.StandardButton.Discard

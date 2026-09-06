"""File actions for the editor window: open, save, recovery, recent files,
and the catalogs a map's name selects."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QTimer
from PySide6.QtGui import QAction
from PySide6.QtWidgets import QFileDialog, QMessageBox

from .constants import (
    DEFAULT_GRID_COLS,
    DEFAULT_GRID_ROWS,
    MAPS_DIR,
    load_actor_kinds,
    load_map_barrier_kinds,
    load_map_bridge_kinds,
    load_map_wall_width_cells,
)
from .dialogs import ResizeMapDialog
from .io import empty_map, load_materials_catalog, read_map


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
        self.adopt_map(None)

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
            self.forget_nested_map_shapes()
            errors = self.validate(loaded, map_name=path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Open Failed", str(exc))
            return
        # Surface structural issues at load time instead of waiting for the
        # user to discover them on save. We still let them load (so they
        # can edit and fix), but the modal makes the problems visible.
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
        self.adopt_map(path.stem)
        self._record_recent_path(path)
        QTimer.singleShot(0, self.maybe_recover_autosave)

    def save(self) -> bool:
        if self.path is None:
            return self.save_as()
        return self._save_to(self.path)

    def _save_to(self, path: Path) -> bool:
        self.forget_nested_map_shapes()
        try:
            errors = self.validate(self.map_data, map_name=path.stem)
        except Exception as exc:
            QMessageBox.critical(self, "Save Failed", str(exc))
            return False
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
        # Save As changes the map's name, and with it its catalogs; the view
        # stays where it is.
        self.barrier_kind_colors = load_map_barrier_kinds(path.stem)
        self.bridge_kind_colors = load_map_bridge_kinds(path.stem)
        self.wall_width_cells = load_map_wall_width_cells(path.stem)
        self._record_recent_path(self.path)
        self.forget_nested_map_shapes()
        self.refresh_ui()
        return True

    def save_as(self) -> bool:
        path, _ = QFileDialog.getSaveFileName(self, "Save Map As", str(self.path or MAPS_DIR), "JSON files (*.json)")
        if not path:
            return False
        return self._save_to(Path(path))

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

    # === Autosave / crash recovery ===

    AUTOSAVE_INTERVAL_MS = 15_000

    def maybe_recover_autosave(self) -> None:
        """Offer to restore a `<file>.autosave.json` sibling newer than the
        opened file. Asked once the window is on screen: a prompt raised
        before the app has a visible window opens behind whatever is in
        front, and the editor looks as if it were doing nothing."""
        if not self.doc.has_recoverable_autosave():
            self.review_repairs(quiet=True)
            return
        autosave = self.doc.autosave_path()
        box = QMessageBox(self)
        box.setWindowTitle("Recover Autosave?")
        box.setText(f"An autosave exists at {autosave.name} that is newer than {self.doc.path.name}. Recover it?")
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)
        box.setDefaultButton(QMessageBox.StandardButton.Yes)
        if box.exec() != QMessageBox.StandardButton.Yes:
            # Declined: the autosave is rejected work; drop it so the next
            # launch doesn't offer it again.
            self.doc.clear_autosave()
            self.review_repairs(quiet=True)
            return
        if self.doc.recover_autosave():
            self.current_level = 0
            self.refresh_ui()
        self.review_repairs(quiet=True)

    def review_repairs(self, *, quiet: bool = False) -> None:
        repaired, summary = self.doc.proposed_repairs()
        if not summary:
            if not quiet:
                QMessageBox.information(self, "Map Repairs", "No automatic repairs are needed. Other issues can be edited from Map Issues.")
            return
        box = QMessageBox(self)
        box.setWindowTitle("Review Map Repairs")
        box.setText("The map contains records that need repair. Apply these changes as one undoable edit?")
        box.setInformativeText("\n".join(summary[:12]))
        box.setDetailedText("\n".join(summary))
        box.setStandardButtons(QMessageBox.StandardButton.Apply | QMessageBox.StandardButton.Cancel)
        box.setDefaultButton(QMessageBox.StandardButton.Cancel)
        if box.exec() == QMessageBox.StandardButton.Apply:
            self.doc.apply_change("Repair Map", repaired, repair=True)

    def recover_unsaved_map(self) -> None:
        if not self.confirm_discard_changes():
            return
        path, _ = QFileDialog.getOpenFileName(self, "Recover Unsaved Map", str(self.doc.recovery_dir), "Autosaved maps (*.autosave.json)")
        if not path:
            return
        try:
            recovered = self.doc.recover_session(Path(path))
        except Exception as exc:
            QMessageBox.warning(self, "Recovery Failed", str(exc))
            return
        if not recovered:
            QMessageBox.information(self, "Map In Use", "That recovery file belongs to an editor that is still running.")
            return
        self.adopt_map(None)
        self.review_repairs(quiet=True)

    def _tick_autosave(self) -> None:
        self.doc.write_autosave()

    def _clear_autosave(self) -> None:
        self.doc.clear_autosave()

    # === Recent files ===

    RECENT_FILES_KEY = "recent_files"
    RECENT_FILES_MAX = 5

    def _load_recent_paths(self) -> list[str]:
        raw = self.preferences.value(self.RECENT_FILES_KEY) or []
        # QSettings on some platforms unwraps single-element lists to scalars.
        if isinstance(raw, str):
            return [raw]
        return [str(p) for p in raw]

    def _record_recent_path(self, path: Path) -> None:
        canonical = str(Path(path).resolve())
        recents = [p for p in self._load_recent_paths() if p != canonical]
        recents.insert(0, canonical)
        del recents[self.RECENT_FILES_MAX :]
        self.preferences.setValue(self.RECENT_FILES_KEY, recents)
        self._rebuild_recent_menu()

    def _rebuild_recent_menu(self) -> None:
        self.recent_menu.clear()
        recents = self._load_recent_paths()
        if not recents:
            empty = QAction("(empty)", self)
            empty.setEnabled(False)
            self.recent_menu.addAction(empty)
            return
        for entry in recents:
            action = QAction(entry, self)
            action.triggered.connect(lambda _checked=False, p=entry: self._open_recent_path(p))
            self.recent_menu.addAction(action)

    def _open_recent_path(self, path_str: str) -> None:
        if not self.confirm_discard_changes():
            return
        candidate = Path(path_str)
        if not candidate.exists():
            self.notify(f"Recent file missing: {candidate}")
            # Drop the dead entry so the user doesn't keep tripping on it.
            remaining = [p for p in self._load_recent_paths() if p != path_str]
            self.preferences.setValue(self.RECENT_FILES_KEY, remaining)
            self._rebuild_recent_menu()
            return
        # Re-use the same load path as File → Open so validation, mtime
        # tracking, and undo-clear all run.
        self.load_path(candidate)

    def reload_dependencies(self) -> None:
        self.forget_nested_map_shapes()
        try:
            self.actor_kinds = load_actor_kinds()
            self.materials_catalog = load_materials_catalog()
            if self.current_material not in self.materials_catalog:
                self.current_material = next(iter(self.materials_catalog), "")
            self.barrier_kind_colors = load_map_barrier_kinds(self.edited_map_name())
            self.bridge_kind_colors = load_map_bridge_kinds(self.edited_map_name())
            self.wall_width_cells = load_map_wall_width_cells(self.edited_map_name())
        except (OSError, ValueError, KeyError) as exc:
            self.notify(f"Catalog reload failed: {exc}")
        self.refresh_ui()

"""The map being edited, independent of any UI.

`MapDocument` owns file identity, map data, the dirty flag, undo history,
and persistence (load/save/autosave). Widgets and dialogs stay in the
window/mixins; everything here is prompt-free so it can be exercised
without Qt widgets.
"""

from __future__ import annotations

import copy
from pathlib import Path
from uuid import uuid4

from PySide6.QtCore import QLockFile, QObject, QStandardPaths, Signal
from PySide6.QtGui import QUndoStack

from .commands import SetMapCommand
from .io import empty_map, read_map, write_map
from .normalization import canonicalize_map, normalize_map
from .repairs import maintain_edit, repair_summary


class MapDocument(QObject):
    changed = Signal(object)
    saved = Signal()
    # Cap the undo history. Each command deep-clones the whole map, so on a
    # large map a long session would accumulate hundreds of MB. 200 steps
    # is plenty of headroom for any plausible undo chain.
    UNDO_LIMIT = 200

    def __init__(self, path: Path | None, *, recovery_dir: Path | None = None):
        super().__init__()
        self.recovery_dir = recovery_dir or Path(QStandardPaths.writableLocation(QStandardPaths.StandardLocation.AppLocalDataLocation)) / "recovery"
        self.session_path = self.recovery_dir / f"untitled-{uuid4().hex}.autosave.json"
        self.recovery_lock: QLockFile | None = None
        self.path: Path | None = path
        if path is not None and path.exists():
            self.map_data: dict = read_map(path)
            # mtime snapshot for external-modification detection. Compared on
            # save so the editor warns before overwriting a file that changed
            # under it (e.g. someone edited `map.json` in another tool, or git
            # pulled).
            self.path_mtime: float | None = path.stat().st_mtime
        else:
            self.map_data = empty_map()
            self.path_mtime = None
        self._saved_data = copy.deepcopy(self.map_data) if self.path_mtime is not None else None
        self.dirty = self._saved_data is None
        self.undo_stack = QUndoStack(self)
        self.undo_stack.setUndoLimit(self.UNDO_LIMIT)

    def set_data(self, map_data: dict, mark_dirty: bool) -> None:
        before = self.map_data
        self.map_data = copy.deepcopy(map_data)
        if mark_dirty:
            self.dirty = self.map_data != self._saved_data
        self.changed.emit(before)

    def apply_change(self, label: str, after: dict, *, repair: bool = False) -> bool:
        after = canonicalize_map(after) if repair else maintain_edit(self.map_data, after)
        # A file may be in any record order; an edit that only reorders is
        # not an edit.
        if after == maintain_edit(self.map_data, self.map_data):
            return False
        self.undo_stack.push(SetMapCommand(self, label, self.map_data, after))
        return True

    def proposed_repairs(self) -> tuple[dict, list[str]]:
        repaired = canonicalize_map(self.map_data)
        return repaired, repair_summary(self.map_data, repaired)

    def replace_with_new(self, map_data: dict) -> None:
        """Adopt a fresh map with no backing file (File → New)."""
        before = self.map_data
        self.clear_autosave()
        self.map_data = normalize_map(map_data)
        self._saved_data = None
        self.path = None
        self.path_mtime = None
        # Fresh map = unsaved by definition; dropping the asterisk would be
        # misleading until the user picks a destination.
        self.dirty = True
        self.undo_stack.clear()
        self.changed.emit(before)

    # === Persistence ===

    def load(self, path: Path, loaded: dict | None = None, path_mtime: float | None = None) -> None:
        """Adopt `path` as the new backing file. Raises on read failure."""
        mtime = path.stat().st_mtime if path_mtime is None else path_mtime
        data = read_map(path) if loaded is None else loaded
        before = self.map_data
        self.clear_autosave()
        self.map_data = data
        self.path = path
        self.path_mtime = mtime
        self._saved_data = copy.deepcopy(data)
        self.dirty = False
        self.undo_stack.clear()
        self.changed.emit(before)

    def externally_modified(self) -> bool:
        # No recorded baseline (fresh map / Save As to a new path) means any
        # existing file at this path is something the user chose to overwrite.
        if self.path is None or self.path_mtime is None:
            return False
        return not self.path.exists() or self.path.stat().st_mtime != self.path_mtime

    def write(self, path: Path | None = None) -> None:
        """Write to the backing file. Raises on write failure."""
        destination = path if path is not None else self.path
        assert destination is not None, "write called with no backing file"
        write_map(destination, self.map_data)
        mtime = destination.stat().st_mtime
        self.clear_autosave()
        self.path = destination
        self.path_mtime = mtime
        self._saved_data = copy.deepcopy(self.map_data)
        self.dirty = False
        self.undo_stack.setClean()
        self.saved.emit()

    # === Autosave / crash recovery ===

    def autosave_path(self) -> Path | None:
        if self.path is None:
            return self.session_path
        return self.path.with_suffix(".autosave.json")

    def write_autosave(self) -> None:
        if not self.dirty:
            return
        autosave = self.autosave_path()
        if autosave is None:
            return
        try:
            if self.path is None and self.recovery_lock is None:
                self.recovery_dir.mkdir(parents=True, exist_ok=True)
                lock = QLockFile(str(autosave) + ".lock")
                if not lock.tryLock(0):
                    return
                self.recovery_lock = lock
            write_map(autosave, self.map_data)
        except Exception:
            # Autosave is best-effort; never interrupt the user with a modal.
            pass

    def clear_autosave(self) -> None:
        autosave = self.autosave_path()
        if autosave is not None and autosave.exists():
            try:
                autosave.unlink()
            except OSError:
                pass
        if self.recovery_lock is not None:
            self.recovery_lock.unlock()
            self.recovery_lock = None

    def has_recoverable_autosave(self) -> bool:
        """True when a `<file>.autosave.json` sibling is newer than the
        backing file."""
        if self.path is None or not self.path.exists():
            return False
        autosave = self.autosave_path()
        if autosave is None or not autosave.exists():
            return False
        try:
            return autosave.stat().st_mtime > self.path.stat().st_mtime
        except OSError:
            return False

    def recover_autosave(self) -> bool:
        """Adopt the autosave's contents; returns False if it can't be read."""
        autosave = self.autosave_path()
        if autosave is None:
            return False
        try:
            recovered = read_map(autosave)
        except Exception:
            return False
        before = self.map_data
        self.map_data = recovered
        # Unsaved by definition until the user writes the real file.
        self.dirty = recovered != self._saved_data
        self.undo_stack.clear()
        self.changed.emit(before)
        return True

    def recover_session(self, path: Path) -> bool:
        lock = QLockFile(str(path) + ".lock")
        if not lock.tryLock(0):
            return False
        try:
            recovered = read_map(path)
        except Exception:
            lock.unlock()
            raise
        self.replace_with_new(recovered)
        self.session_path = path
        self.recovery_lock = lock
        return True

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
from .io import read_map, write_map
from .normalization import canonicalize_map, empty_map, normalize_map
from .repairs import repair_summary


class MapDocument(QObject):
    changed = Signal(object)
    replaced = Signal()
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
        self.active_map: str | None = None
        self._canonical: dict[str | None, dict] = {}
        self._repairs_pending: dict[str | None, bool] = {}
        if path is not None and path.exists():
            self.root_data = read_map(path)
            # mtime snapshot for external-modification detection. Compared on
            # save so the editor warns before overwriting a file that changed
            # under it (e.g. someone edited `map.json` in another tool, or git
            # pulled).
            self.path_mtime: float | None = path.stat().st_mtime
        else:
            self.root_data = empty_map()
            self.path_mtime = None
        self._saved_data = copy.deepcopy(self.root_data) if self.path_mtime is not None else None
        self.dirty = self._saved_data is None
        self.undo_stack = QUndoStack(self)
        self.undo_stack.setUndoLimit(self.UNDO_LIMIT)

    @property
    def root_data(self) -> dict:
        return self._root_data

    # Replacing the document forgets the canonical forms memoized below.
    @root_data.setter
    def root_data(self, data: dict) -> None:
        self._root_data = data
        self._canonical.clear()
        self._repairs_pending.clear()

    @property
    def map_data(self) -> dict:
        if self.active_map is None:
            return self.root_data
        return self.root_data["nested_geometry"][self.active_map]

    # The active map canonicalized, and whether that changes any record,
    # memoized until `root_data` is replaced: every edit asks for both, and
    # canonicalizing a large map per click would lag.
    def canonical_map_data(self) -> dict:
        if self.active_map not in self._canonical:
            self._canonical[self.active_map] = canonicalize_map(self.map_data)
        return self._canonical[self.active_map]

    def repairs_pending(self) -> bool:
        if self.active_map not in self._repairs_pending:
            self._repairs_pending[self.active_map] = bool(repair_summary(self.map_data, self.canonical_map_data()))
        return self._repairs_pending[self.active_map]

    # Geometry transforms can move invalid records; only explicit repair may remove them.
    def maintain(self, after: dict) -> dict:
        normalized = normalize_map(after)
        return normalized if self.repairs_pending() else canonicalize_map(normalized)

    @property
    def nested_geometry(self) -> dict:
        return self.root_data.get("nested_geometry", {})

    def select_map(self, name: str | None) -> None:
        if name is not None and name not in self.nested_geometry:
            raise ValueError(f"No nested geometry named {name!r}")
        if name == self.active_map:
            return
        before = self.map_data
        self.active_map = name
        self.changed.emit(before)

    def set_data(self, map_data: dict, mark_dirty: bool, active_map: str | None = None) -> None:
        before = self.map_data
        self.root_data = copy.deepcopy(map_data)
        self.active_map = active_map if active_map in self.nested_geometry else None
        if mark_dirty:
            self.dirty = self.root_data != self._saved_data
        self.changed.emit(before)

    def apply_root_change(self, label: str, after: dict, active_map: str | None) -> bool:
        if after == self.root_data:
            return False
        self.undo_stack.push(SetMapCommand(self, label, self.root_data, after, active_map))
        return True

    def apply_change(self, label: str, after: dict) -> bool:
        after = self.maintain(after)
        current = normalize_map(self.map_data) if self.repairs_pending() else self.canonical_map_data()
        if after == current:
            return False
        root = after
        if self.active_map is not None:
            root = copy.deepcopy(self.root_data)
            root["nested_geometry"][self.active_map] = after
        return self.apply_root_change(label, root, self.active_map)

    def proposed_repairs(self) -> tuple[dict, list[str]]:
        """The whole document repaired, the outer map and every nested
        definition, with the summary lines of each."""
        repaired = canonicalize_map(self.root_data)
        summary = repair_summary(self.root_data, repaired)
        for name, geometry in self.nested_geometry.items():
            repaired["nested_geometry"][name] = canonicalize_map(geometry)
            summary.extend(f"Nested {name}: {line}" for line in repair_summary(geometry, repaired["nested_geometry"][name]))
        return repaired, summary

    def apply_repairs(self, repaired: dict) -> bool:
        return self.apply_root_change("Repair Map", repaired, self.active_map)

    def replace_with_new(self, map_data: dict, path: Path | None = None) -> None:
        """Adopt an unsaved map at a chosen destination (None for a recovered session)."""
        path_mtime = path.stat().st_mtime if path is not None and path.exists() else None
        before = self.map_data
        self.active_map = None
        self.clear_autosave()
        self.root_data = normalize_map(map_data)
        self._saved_data = None
        self.path = path
        self.path_mtime = path_mtime
        # Fresh map = unsaved by definition; dropping the asterisk would be
        # misleading until it is written.
        self.dirty = True
        self.undo_stack.clear()
        self.replaced.emit()
        self.changed.emit(before)

    # === Persistence ===

    def load(self, path: Path, loaded: dict | None = None, path_mtime: float | None = None) -> None:
        """Adopt `path` as the new backing file. Raises on read failure."""
        mtime = path.stat().st_mtime if path_mtime is None else path_mtime
        data = read_map(path) if loaded is None else loaded
        before = self.map_data
        self.active_map = None
        self.clear_autosave()
        self.root_data = data
        self.path = path
        self.path_mtime = mtime
        self._saved_data = copy.deepcopy(data)
        self.dirty = False
        self.undo_stack.clear()
        self.replaced.emit()
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
        write_map(destination, self.root_data)
        mtime = destination.stat().st_mtime
        self.clear_autosave()
        self.path = destination
        self.path_mtime = mtime
        self._saved_data = copy.deepcopy(self.root_data)
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
            write_map(autosave, self.root_data)
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
        self.active_map = None
        self.root_data = recovered
        self.dirty = recovered != self._saved_data
        self.undo_stack.clear()
        self.replaced.emit()
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

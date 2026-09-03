"""The map being edited, independent of any UI.

`MapDocument` owns file identity, map data, the dirty flag, undo history,
and persistence (load/save/autosave). Widgets and dialogs stay in the
window/mixins; everything here is prompt-free so it can be exercised
without Qt widgets.
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtGui import QUndoStack

from .io import empty_map, read_map, write_map
from .normalization import canonicalize_map


class MapDocument:
    # Cap the undo history. Each command deep-clones the whole map, so on a
    # large map a long session would accumulate hundreds of MB. 200 steps
    # is plenty of headroom for any plausible undo chain.
    UNDO_LIMIT = 200

    def __init__(self, path: Path | None):
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
        self.dirty = False
        self.undo_stack = QUndoStack()
        self.undo_stack.setUndoLimit(self.UNDO_LIMIT)

    # === Data ===

    @property
    def barrier_kinds(self) -> list[str]:
        return self.map_data.get("barrier_kinds", [])

    def set_data(self, map_data: dict, mark_dirty: bool) -> None:
        self.map_data = canonicalize_map(map_data)
        if mark_dirty:
            self.dirty = True

    def replace_with_new(self, map_data: dict) -> None:
        """Adopt a fresh map with no backing file (File → New)."""
        self.map_data = map_data
        self.path = None
        self.path_mtime = None
        # Fresh map = unsaved by definition; dropping the asterisk would be
        # misleading until the user picks a destination.
        self.dirty = True
        self.undo_stack.clear()

    # === Persistence ===

    def load(self, path: Path) -> None:
        """Adopt `path` as the new backing file. Raises on read failure."""
        self.map_data = read_map(path)
        self.path = path
        self.path_mtime = path.stat().st_mtime
        self.dirty = False
        self.undo_stack.clear()

    def externally_modified(self) -> bool:
        # No recorded baseline (fresh map / Save As to a new path) means any
        # existing file at this path is something the user chose to overwrite.
        if self.path is None or self.path_mtime is None or not self.path.exists():
            return False
        return self.path.stat().st_mtime > self.path_mtime + 1e-3

    def write(self) -> None:
        """Write to the backing file. Raises on write failure."""
        assert self.path is not None, "write called with no backing file"
        write_map(self.path, self.map_data)
        self.path_mtime = self.path.stat().st_mtime
        self.dirty = False
        self.clear_autosave()

    # === Autosave / crash recovery ===

    def autosave_path(self) -> Path | None:
        if self.path is None:
            return None
        return self.path.with_suffix(".autosave.json")

    def write_autosave(self) -> None:
        if not self.dirty:
            return
        autosave = self.autosave_path()
        if autosave is None:
            return
        try:
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
        self.map_data = recovered
        # Unsaved by definition until the user writes the real file.
        self.dirty = True
        return True

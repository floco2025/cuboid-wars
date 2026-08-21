"""Main map editor window."""

from __future__ import annotations

import copy
from pathlib import Path

from PySide6.QtCore import QSettings, Qt, QTimer
from PySide6.QtGui import QAction, QFont, QKeySequence, QShortcut, QStandardItem, QStandardItemModel, QUndoStack
from PySide6.QtWidgets import QComboBox, QLabel, QMainWindow, QMenu, QToolBar

from .canvas import Canvas
from .commands import SetMapCommand
from .constants import (
    BARRIER_KIND_TABLE,
    DEFAULT_ACTOR_COUNT,
    DEFAULT_ALIAS,
    ERASE_MODES,
    ITEM_TYPES,
    MODE_CATEGORIES,
    MODE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_DOWN,
    MODE_RAMP_UP,
    MODE_SPAWN_ZONE_EDIT,
    MODES,
    STATUS_TIMEOUT_MS,
)
from .document import MapDocument
from .erase import EraseMixin
from .file_actions import FileActionsMixin
from .display import level_label
from .normalization import canonicalize_map
from .io import load_materials_catalog
from .items import ItemsMixin
from .ladders import LaddersMixin
from .lights import LightsMixin
from .placement import PlacementMixin
from .spawn_zones import SpawnZoneEditMixin
from .structure import StructureMixin
from .types import SpawnZoneDrag, ZoneRef
from .validation import validate_map


class EditorWindow(
    FileActionsMixin,
    PlacementMixin,
    ItemsMixin,
    LightsMixin,
    LaddersMixin,
    EraseMixin,
    StructureMixin,
    SpawnZoneEditMixin,
    QMainWindow,
):
    def __init__(self, path: Path):
        super().__init__()
        # The document is the map being edited (data, file identity, dirty
        # state, undo history); the window holds view/tool state and widgets.
        self.doc = MapDocument(path)
        # If a newer autosave sits next to the file we just opened, offer to
        # recover it. Done before any UI is built so the user sees their
        # restored work as the initial state.
        self._maybe_recover_autosave()
        self.current_level = 0
        self.mode = MODE_FLOOR
        self.shortcuts = []
        # No default kind: the dialog opens with Kind blank on the first paint
        # of a session and remembers the last value across subsequent paints.
        self.recent_actor_spawn_kind: str = ""
        self.recent_actor_spawn_count: int = DEFAULT_ACTOR_COUNT
        # Last-used barrier kind, seeded from the first configured kind so the
        # picker dialog has a sensible default on the first paint.
        self.recent_barrier_kind: str | None = BARRIER_KIND_TABLE[0] if BARRIER_KIND_TABLE else None
        self.recent_pressure_plate_kind: str | None = BARRIER_KIND_TABLE[0] if BARRIER_KIND_TABLE else None
        self.recent_item_type: str = ITEM_TYPES[0]
        self.recent_item_key_kind: str | None = BARRIER_KIND_TABLE[0] if BARRIER_KIND_TABLE else None
        # (row_spacing, row_offset, col_spacing, col_offset) — remembered
        # across opens of the Auto-Place Lights dialog. Spacing is "cells
        # skipped between lights": 0 = every cell, 1 = every other, 2 = every
        # third.
        self.recent_auto_place_lights: tuple[int, int, int, int] = (0, 0, 0, 0)
        self.recent_ladder_levels: int = 1
        # `(level_idx, [light, ...])` while an Auto-Place Lights confirmation
        # is pending; canvas paints these as ghosts. `None` outside the
        # preview window.
        self.pending_auto_lights: tuple[int, list[dict]] | None = None
        self.selected_spawn_zone_ref: ZoneRef | None = None
        self.spawn_zone_drag: SpawnZoneDrag | None = None
        self.show_material_overlay = False
        # Show prev/next level geometry as ghosted overlays — helps when
        # placing ramps that span two levels.
        self.show_adjacent_levels = False
        # Material used as the default for newly painted floors / walls / ramps.
        # The user picks a different one from the materials palette.
        # Default for newly-painted segments. Must be an alias (face values
        # are validated against `MATERIAL_ALIASES` on save) — see
        # `DEFAULT_ALIAS` in constants.py for the selection rule.
        self.current_material: str = DEFAULT_ALIAS
        self.materials_catalog: list[str] = load_materials_catalog()

        self.canvas = Canvas(self)
        self.canvas.setCursor(self._cursor_for_mode(self.mode))
        self.setCentralWidget(self.canvas)
        self.setWindowTitle("Cuboid Wars Editor")

        self.level_combo = QComboBox()
        self.level_combo.currentIndexChanged.connect(self.select_level)
        self.mode_combo = self._build_mode_combo()
        self.mode_combo.currentTextChanged.connect(self.set_mode)
        # Status bar: left = persistent last-action message, right = red
        # structural-issue badge (hidden when the map is valid).
        self.last_action_label = QLabel()
        self.status_label = QLabel()
        # Track the previous undo-stack index so transitions can be labelled
        # "Undid X" / "Redid X" instead of just showing the new tip.
        self._undo_index_seen = 0
        self.undo_stack.indexChanged.connect(self._on_undo_index_changed)

        self.build_menus()
        self.build_toolbar()
        self.statusBar().addWidget(self.last_action_label, 1)
        self.statusBar().addPermanentWidget(self.status_label)
        self.refresh_ui()
        self.resize_to_map()

        # Autosave timer — periodically writes a `.autosave.json` sibling when
        # the map is dirty so a crash/kill doesn't lose work.
        self._autosave_timer = QTimer(self)
        self._autosave_timer.setInterval(self.AUTOSAVE_INTERVAL_MS)
        self._autosave_timer.timeout.connect(self._tick_autosave)
        self._autosave_timer.start()

    # === Document delegation ===
    # The mixins predate `MapDocument` and address document state through the
    # window; these properties keep that surface while the document owns it.

    @property
    def map_data(self) -> dict:
        return self.doc.map_data

    @map_data.setter
    def map_data(self, value: dict) -> None:
        self.doc.map_data = value

    @property
    def dirty(self) -> bool:
        return self.doc.dirty

    @dirty.setter
    def dirty(self, value: bool) -> None:
        self.doc.dirty = value

    @property
    def path(self) -> Path | None:
        return self.doc.path

    @path.setter
    def path(self, value: Path | None) -> None:
        self.doc.path = value

    @property
    def path_mtime(self) -> float | None:
        return self.doc.path_mtime

    @path_mtime.setter
    def path_mtime(self, value: float | None) -> None:
        self.doc.path_mtime = value

    @property
    def undo_stack(self) -> QUndoStack:
        return self.doc.undo_stack

    # === Menus & toolbar ===

    def _build_mode_combo(self) -> QComboBox:
        # Each category contributes a disabled header row followed by its
        # modes. Tool descriptions live in Help → Tool Reference.
        combo = QComboBox()
        model = QStandardItemModel(combo)
        header_font = QFont()
        header_font.setBold(True)
        for label, modes in MODE_CATEGORIES:
            header = QStandardItem(f"— {label} —")
            header.setEnabled(False)
            header.setSelectable(False)
            header.setFont(header_font)
            model.appendRow(header)
            for mode in modes:
                model.appendRow(QStandardItem(mode))
        combo.setModel(model)
        # Headers aren't selectable; first selectable item is row 1.
        combo.setCurrentIndex(1)
        return combo

    # Map each mode to the cursor it should display so a peripheral glance
    # tells the user which tool is active without reading the toolbar.
    def _cursor_for_mode(self, mode: str) -> Qt.CursorShape:
        if mode in (MODE_LIGHT, MODE_LADDER, MODE_PRESSURE_PLATE, MODE_ITEM):
            return Qt.CursorShape.PointingHandCursor
        if mode == MODE_SPAWN_ZONE_EDIT:
            return Qt.CursorShape.OpenHandCursor
        if mode in ERASE_MODES:
            return Qt.CursorShape.ForbiddenCursor
        return Qt.CursorShape.CrossCursor

    def build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&File")
        self.add_menu_action(file_menu, "&New...", QKeySequence.StandardKey.New, self.new_file)
        self.add_menu_action(file_menu, "&Open...", QKeySequence.StandardKey.Open, self.open_file)
        self.recent_menu = file_menu.addMenu("Open &Recent")
        self._rebuild_recent_menu()
        # Track the initial path as a recent so it shows up next launch.
        if self.path is not None and self.path.exists():
            self._record_recent_path(self.path)
        self.add_menu_action(file_menu, "&Save", QKeySequence.StandardKey.Save, self.save)
        self.add_menu_action(file_menu, "Save &As...", QKeySequence.StandardKey.SaveAs, self.save_as)
        file_menu.addSeparator()
        self.add_menu_action(file_menu, "&Quit", QKeySequence.StandardKey.Quit, self.close)

        edit_menu = self.menuBar().addMenu("&Edit")
        undo_action = self.undo_stack.createUndoAction(self, "&Undo")
        undo_action.setShortcuts(QKeySequence.StandardKey.Undo)
        edit_menu.addAction(undo_action)
        redo_action = self.undo_stack.createRedoAction(self, "&Redo")
        redo_action.setShortcuts(QKeySequence.StandardKey.Redo)
        edit_menu.addAction(redo_action)
        edit_menu.addSeparator()
        self.add_menu_action(edit_menu, "Resi&ze Map...", None, self.resize_map)
        edit_menu.addSeparator()
        self.add_menu_action(edit_menu, "&Add Level", None, self.add_level)
        self.add_menu_action(edit_menu, "Re&name Level...", None, self.rename_level)
        self.add_menu_action(edit_menu, "Re&move Level", None, self.remove_level)
        edit_menu.addSeparator()
        self.add_menu_action(edit_menu, "Auto-Place &Lights...", None, self.open_auto_place_lights_dialog)
        self.add_menu_action(edit_menu, "&Clear Lights On Level", None, self.clear_lights_on_current_level)

        view_menu = self.menuBar().addMenu("&View")
        self.material_overlay_action = QAction("Show &Material Overlay", self)
        self.material_overlay_action.setCheckable(True)
        self.material_overlay_action.setShortcut(QKeySequence("M"))
        self.material_overlay_action.toggled.connect(self.set_material_overlay)
        view_menu.addAction(self.material_overlay_action)
        self.adjacent_levels_action = QAction("Show &Adjacent Levels", self)
        self.adjacent_levels_action.setCheckable(True)
        self.adjacent_levels_action.setShortcut(QKeySequence("Shift+M"))
        self.adjacent_levels_action.toggled.connect(self.set_adjacent_levels)
        view_menu.addAction(self.adjacent_levels_action)

        help_menu = self.menuBar().addMenu("&Help")
        self.add_menu_action(help_menu, "Tool &Reference", None, self.show_tool_reference)

        self.add_shortcut(Qt.Key.Key_Up, self.next_level)
        self.add_shortcut(Qt.Key.Key_Down, self.previous_level)
        self.add_shortcut(Qt.Key.Key_Left, self.previous_tool)
        self.add_shortcut(Qt.Key.Key_Right, self.next_tool)

    def add_shortcut(self, key, callback) -> None:
        shortcut = QShortcut(QKeySequence(key), self)
        shortcut.setContext(Qt.ShortcutContext.WindowShortcut)
        shortcut.activated.connect(callback)
        self.shortcuts.append(shortcut)

    def add_menu_action(self, menu: QMenu, text: str, shortcut, callback) -> QAction:
        action = QAction(text, self)
        if shortcut is not None:
            action.setShortcut(shortcut)
        action.triggered.connect(callback)
        menu.addAction(action)
        return action

    # === Autosave / crash recovery ===

    AUTOSAVE_INTERVAL_MS = 60_000

    def _maybe_recover_autosave(self) -> None:
        # Called once, very early in __init__ (before UI is constructed). If a
        # `<file>.autosave.json` sibling is newer than the file we just
        # loaded, offer to restore it.
        if not self.doc.has_recoverable_autosave():
            return
        autosave = self.doc.autosave_path()
        from PySide6.QtWidgets import QMessageBox  # local import; avoids top-level cycle

        response = QMessageBox.question(
            self,
            "Recover Autosave?",
            f"An autosave exists at {autosave.name} that is newer than {self.doc.path.name}. Recover it?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.Yes,
        )
        if response != QMessageBox.StandardButton.Yes:
            # Declined: the autosave is rejected work; drop it so the next
            # launch doesn't offer it again.
            self.doc.clear_autosave()
            return
        self.doc.recover_autosave()

    def _tick_autosave(self) -> None:
        self.doc.write_autosave()

    def _clear_autosave(self) -> None:
        self.doc.clear_autosave()

    # === Recent files ===

    RECENT_FILES_KEY = "recent_files"
    RECENT_FILES_MAX = 5

    def _load_recent_paths(self) -> list[str]:
        settings = QSettings()
        raw = settings.value(self.RECENT_FILES_KEY) or []
        # QSettings on some platforms unwraps single-element lists to scalars.
        if isinstance(raw, str):
            return [raw]
        return [str(p) for p in raw]

    def _record_recent_path(self, path: Path) -> None:
        canonical = str(Path(path).resolve())
        recents = [p for p in self._load_recent_paths() if p != canonical]
        recents.insert(0, canonical)
        del recents[self.RECENT_FILES_MAX :]
        QSettings().setValue(self.RECENT_FILES_KEY, recents)
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
            self._flash_status(f"Recent file missing: {candidate}")
            # Drop the dead entry so the user doesn't keep tripping on it.
            remaining = [p for p in self._load_recent_paths() if p != path_str]
            QSettings().setValue(self.RECENT_FILES_KEY, remaining)
            self._rebuild_recent_menu()
            return
        # Re-use the same load path as File → Open so validation, mtime
        # tracking, and undo-clear all run.
        self.load_path(candidate)

    def build_toolbar(self) -> None:
        toolbar = QToolBar("Tools", self)
        toolbar.setMovable(False)
        toolbar.addWidget(QLabel("Level "))
        toolbar.addWidget(self.level_combo)
        toolbar.addSeparator()
        toolbar.addWidget(QLabel("Tool "))
        toolbar.addWidget(self.mode_combo)
        # Persistent "Building UP/DOWN" hint that disambiguates the two ramp
        # modes mid-drag. Hidden outside ramp modes so it doesn't clutter the
        # toolbar.
        self.ramp_direction_label = QLabel()
        self.ramp_direction_label.setStyleSheet("color: #fbbf24; padding: 0 8px;")
        toolbar.addWidget(self.ramp_direction_label)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, toolbar)

    # === State updates & UI refresh ===

    def set_map(self, map_data: dict, mark_dirty: bool) -> None:
        prior_selection: tuple[str, dict] | None = None
        if self.selected_spawn_zone_ref is not None:
            ref = self.selected_spawn_zone_ref
            if 0 <= ref.index < len(self.map_data[ref.list_name]):
                prior_selection = (ref.list_name, copy.deepcopy(self.map_data[ref.list_name][ref.index]))
        self.doc.set_data(map_data, mark_dirty)
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        if prior_selection is not None:
            list_name, snapshot = prior_selection
            self.selected_spawn_zone_ref = self._zone_ref_after_change(list_name, snapshot)
        else:
            self.selected_spawn_zone_ref = None
        self.refresh_ui()

    def apply_change(self, label: str, after: dict) -> None:
        before = self.map_data
        if canonicalize_map(before) == canonicalize_map(after):
            return
        self.undo_stack.push(SetMapCommand(self, label, before, after))
        self._set_last_action(label)

    def refresh_ui(self) -> None:
        self.level_combo.blockSignals(True)
        self.level_combo.clear()
        for idx, level in enumerate(self.map_data["levels"]):
            self.level_combo.addItem(level_label(level, idx))
        self.level_combo.setCurrentIndex(self.current_level)
        self.level_combo.blockSignals(False)
        self.canvas.update()
        self.update_status()
        suffix = "*" if self.dirty else ""
        file_name = str(self.path) if self.path else "Untitled"
        self.setWindowTitle(f"Cuboid Wars Editor - {file_name}{suffix}")

    def resize_to_map(self) -> None:
        self.canvas.updateGeometry()
        self.resize(self.sizeHint())

    def update_status(self) -> None:
        errors = validate_map(self.map_data)
        if errors:
            self.status_label.setText(f"{len(errors)} structural issue(s)")
            self.status_label.setToolTip("\n".join(errors[:20]))
            self.status_label.setStyleSheet("color: #f87171;")
            self.status_label.setVisible(True)
        else:
            # Hide the validity badge when the map is fine — the green-light
            # "Structurally valid" text just added noise.
            self.status_label.setVisible(False)
        # Ramp-direction hint: only visible in MODE_RAMP_UP / MODE_RAMP_DOWN,
        # otherwise the label is empty (it still occupies the toolbar slot but
        # doesn't show text).
        if self.mode == MODE_RAMP_UP:
            target = self.current_level + 1
            self.ramp_direction_label.setText(f"↑ Building UP to Level {target}")
        elif self.mode == MODE_RAMP_DOWN:
            target = self.current_level - 1
            self.ramp_direction_label.setText(f"↓ Building DOWN to Level {target}")
        else:
            self.ramp_direction_label.setText("")

    def _set_last_action(self, message: str) -> None:
        self.last_action_label.setText(message)

    def _flash_status(self, message: str) -> None:
        # Soft-rejection feedback: pin the message to the last-action label
        # and also raise a transient toast so it's noticed immediately.
        self._set_last_action(message)
        self.statusBar().showMessage(message, STATUS_TIMEOUT_MS)

    def _on_undo_index_changed(self, new_index: int) -> None:
        old_index = self._undo_index_seen
        self._undo_index_seen = new_index
        if new_index < old_index:
            # We undid: the command at `new_index` is what was just reverted.
            cmd = self.undo_stack.command(new_index)
            if cmd is not None:
                self._set_last_action(f"Undid {cmd.text()}")
        elif new_index > old_index:
            # We redid: the command at `new_index - 1` is what was just
            # re-applied. Skip when the bump came from `push` (apply_change
            # already set the label with the action name).
            cmd = self.undo_stack.command(new_index - 1)
            if cmd is not None and self.last_action_label.text() != cmd.text():
                self._set_last_action(f"Redid {cmd.text()}")

    # === Navigation (level / tool selection) ===

    def select_level(self, index: int) -> None:
        if 0 <= index < len(self.map_data["levels"]):
            self.current_level = index
            self.canvas.update()
            # Keep the status bar + ramp-direction label in sync with the
            # new level; otherwise "Building UP to Level N" lags one click.
            self.update_status()

    def set_mode(self, mode: str) -> None:
        self.mode = mode
        self.canvas.setCursor(self._cursor_for_mode(mode))
        self.canvas.update()
        self.update_status()

    def set_material_overlay(self, enabled: bool) -> None:
        self.show_material_overlay = enabled
        self.canvas.update()

    def set_adjacent_levels(self, enabled: bool) -> None:
        self.show_adjacent_levels = enabled
        self.canvas.update()

    def previous_level(self) -> None:
        self.set_level_index(self.current_level - 1)

    def next_level(self) -> None:
        self.set_level_index(self.current_level + 1)

    def set_level_index(self, index: int) -> None:
        clamped = max(0, min(index, len(self.map_data["levels"]) - 1))
        if clamped == self.current_level:
            return
        self.current_level = clamped
        self.refresh_ui()

    def previous_tool(self) -> None:
        self._step_tool(-1)

    def next_tool(self) -> None:
        self._step_tool(1)

    def _step_tool(self, direction: int) -> None:
        # Skip over disabled header rows (the category separators in the
        # grouped picker) so arrow-key cycling visits every real mode and
        # never lands on a header.
        model = self.mode_combo.model()
        count = self.mode_combo.count()
        if count == 0:
            return
        idx = self.mode_combo.currentIndex()
        for _ in range(count):
            idx = (idx + direction) % count
            item = model.item(idx) if hasattr(model, "item") else None
            if item is None or item.isSelectable():
                self.mode_combo.setCurrentIndex(idx)
                return

    # === Close handler ===

    def closeEvent(self, event) -> None:
        if self.confirm_discard_changes():
            event.accept()
        else:
            event.ignore()

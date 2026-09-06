"""Main map editor window."""

from __future__ import annotations

import copy
from pathlib import Path

from PySide6.QtCore import QSettings, Qt, QTimer
from PySide6.QtGui import QAction, QFont, QKeySequence, QShortcut, QStandardItem, QStandardItemModel, QUndoStack
from PySide6.QtWidgets import QComboBox, QFileDialog, QLabel, QMainWindow, QMenu, QMessageBox, QToolBar

from .canvas import Canvas
from .dialogs import NestedMotion
from .constants import (
    DEFAULT_ACTOR_COUNT,
    DEFAULT_ALIAS,
    DEFAULT_WALL_WIDTH_CELLS,
    ERASE_MODES,
    ITEM_TYPES,
    MODE_BRIDGE_PLATE,
    MODE_CATEGORIES,
    MODE_FIREWORK_PLATE,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_SELECT,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_DOWN,
    MODE_RAMP_UP,
    load_map_barrier_kinds,
    load_map_bridge_kinds,
    load_map_wall_width_cells,
    load_actor_kinds,
)
from .document import MapDocument
from .erase import EraseMixin
from .file_actions import FileActionsMixin
from .display import level_label
from .io import load_materials_catalog
from .items import ItemsMixin
from .ladders import LaddersMixin
from .lights import LightsMixin
from .nested_maps import NestedMapsMixin
from .placement import PlacementMixin
from .select import SelectMixin
from .spawn_zones import SpawnZoneEditMixin
from .structure import StructureMixin
from .types import SpawnZoneDrag, ZoneRef
from .validation import validate_map
from .issues import IssuesPanel
from .dependencies import MapDependencies
from .tool_settings import ToolSettings
from .window_geometry import WindowGeometry


class EditorWindow(
    FileActionsMixin,
    PlacementMixin,
    ItemsMixin,
    LightsMixin,
    LaddersMixin,
    NestedMapsMixin,
    EraseMixin,
    StructureMixin,
    SelectMixin,
    SpawnZoneEditMixin,
    QMainWindow,
):
    def __init__(self, path: Path, *, preferences: QSettings | None = None):
        super().__init__()
        self.preferences = preferences if preferences is not None else QSettings()
        # The document is the map being edited (data, file identity, dirty
        # state, undo history); the window holds view/tool state and widgets.
        self.doc = MapDocument(path)
        self.barrier_kind_colors = load_map_barrier_kinds(path.stem)
        self.bridge_kind_colors = load_map_bridge_kinds(path.stem)
        self.wall_width_cells = load_map_wall_width_cells(path.stem)
        self.actor_kinds = load_actor_kinds()
        self.current_level = 0
        self.mode = MODE_SELECT
        self.shortcuts = []
        # No default kind: the dialog opens with Kind blank on the first paint
        # of a session and remembers the last value across subsequent paints.
        self.recent_actor_spawn_kind: str = ""
        self.recent_actor_spawn_count: int = DEFAULT_ACTOR_COUNT
        # Last-used barrier kind, seeded from the map's first listed kind so
        # the picker dialog has a sensible default on the first paint.
        first_kind = self.barrier_kinds[0] if self.barrier_kinds else None
        self.recent_barrier_kind: str | None = first_kind
        self.recent_pressure_plate_kind: str | None = first_kind
        self.recent_item_type: str = ITEM_TYPES[0]
        self.recent_item_key_kind: str | None = first_kind
        first_bridge_kind = self.bridge_kinds[0] if self.bridge_kinds else None
        self.recent_bridge_kind: str | None = first_bridge_kind
        self.recent_bridge_plate_kind: str | None = first_bridge_kind
        # (row_spacing, row_offset, col_spacing, col_offset) — remembered
        # across opens of the Auto-Place Lights dialog. Spacing is "cells
        # skipped between lights": 0 = every cell, 1 = every other, 2 = every
        # third.
        self.recent_auto_place_lights: tuple[int, int, int, int] = (0, 0, 0, 0)
        self.recent_ladder_levels: int = 1
        # The last nested map dialog answer:
        # (map, to_level, travel_secs, pause, phase, from_nudge, to_nudge).
        self.recent_nested_map: NestedMotion | None = None
        self.nested_map_shapes: dict = {}
        # `(level_idx, [light, ...])` while an Auto-Place Lights confirmation
        # is pending; canvas paints these as ghosts. `None` outside the
        # preview window.
        self.pending_auto_lights: tuple[int, list[dict]] | None = None
        self.selected_spawn_zone_ref: ZoneRef | None = None
        self.spawn_zone_drag: SpawnZoneDrag | None = None
        self.tile_selection: tuple[int, int, int, int] | None = None
        self.tile_clipboard: dict | None = None
        self.select_drag_kind: str | None = None
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
        self.issues_panel = IssuesPanel(self)
        self.issues_panel.focused.connect(self.focus_issue)
        self.issues_panel.repair_requested.connect(self.review_repairs)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.issues_panel)
        self.issues_panel.hide()
        self.tool_settings = ToolSettings(self)
        self.issues_action = QAction("Issues", self)
        self.issues_action.triggered.connect(self.issues_panel.show)
        self.issues_action.setVisible(False)

        self.build_menus()
        self.build_toolbar()
        self.doc.changed.connect(self._on_document_changed)
        self.doc.saved.connect(self.refresh_ui)
        self.dependencies = MapDependencies(self)
        self.dependencies.changed.connect(self.reload_dependencies)
        self.refresh_ui()
        self.window_geometry = WindowGeometry(self, self.preferences)
        self.canvas.fit_map()

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
    def barrier_kinds(self) -> list[str]:
        return list(self.barrier_kind_colors)

    @property
    def bridge_kinds(self) -> list[str]:
        return list(self.bridge_kind_colors)

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
        model.appendRow(QStandardItem(MODE_SELECT))
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
        combo.setCurrentIndex(0)
        return combo

    # Map each mode to the cursor it should display so a peripheral glance
    # tells the user which tool is active without reading the toolbar.
    def _cursor_for_mode(self, mode: str) -> Qt.CursorShape:
        if mode == MODE_SELECT:
            return Qt.CursorShape.ArrowCursor
        if mode in (MODE_LIGHT, MODE_LADDER, MODE_PRESSURE_PLATE, MODE_BRIDGE_PLATE, MODE_FIREWORK_PLATE, MODE_ITEM):
            return Qt.CursorShape.PointingHandCursor
        if mode in ERASE_MODES:
            return Qt.CursorShape.ForbiddenCursor
        return Qt.CursorShape.CrossCursor

    def build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&File")
        self.add_menu_action(file_menu, "&New...", QKeySequence.StandardKey.New, self.new_file)
        self.add_menu_action(file_menu, "&Open...", QKeySequence.StandardKey.Open, self.open_file)
        self.add_menu_action(file_menu, "Recover &Unsaved Map...", None, self.recover_unsaved_map)
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
        self.build_selection_actions(edit_menu)
        self.add_menu_action(edit_menu, "Review &Repairs...", None, self.review_repairs)
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
        self.add_menu_action(view_menu, "Zoom &In", QKeySequence.StandardKey.ZoomIn, lambda: self.canvas.zoom_by(1.25))
        self.add_menu_action(view_menu, "Zoom &Out", QKeySequence.StandardKey.ZoomOut, lambda: self.canvas.zoom_by(0.8))
        fit_action = self.add_menu_action(view_menu, "&Fit Map", QKeySequence("F"), self.canvas.fit_map)
        self.canvas_shortcut(fit_action)
        view_menu.addSeparator()
        view_menu.addAction(self.issues_panel.toggleViewAction())
        self.material_overlay_action = QAction("Show &Material Overlay", self)
        self.material_overlay_action.setCheckable(True)
        self.material_overlay_action.setShortcut(QKeySequence("M"))
        self.material_overlay_action.toggled.connect(self.set_material_overlay)
        view_menu.addAction(self.material_overlay_action)
        self.canvas_shortcut(self.material_overlay_action)
        self.adjacent_levels_action = QAction("Show &Adjacent Levels", self)
        self.adjacent_levels_action.setCheckable(True)
        self.adjacent_levels_action.setShortcut(QKeySequence("L"))
        self.adjacent_levels_action.toggled.connect(self.set_adjacent_levels)
        view_menu.addAction(self.adjacent_levels_action)
        self.canvas_shortcut(self.adjacent_levels_action)

        help_menu = self.menuBar().addMenu("&Help")
        self.add_menu_action(help_menu, "Tool &Reference", None, self.show_tool_reference)

        self.add_shortcut(Qt.Key.Key_Up, self.next_level)
        self.add_shortcut(Qt.Key.Key_Down, self.previous_level)
        self.add_shortcut(Qt.Key.Key_Left, self.previous_tool)
        self.add_shortcut(Qt.Key.Key_Right, self.next_tool)

    def add_shortcut(self, key, callback) -> None:
        shortcut = QShortcut(QKeySequence(key), self.canvas)
        shortcut.setContext(Qt.ShortcutContext.WidgetShortcut)
        shortcut.activated.connect(callback)
        self.shortcuts.append(shortcut)

    def canvas_shortcut(self, action: QAction) -> None:
        action.setShortcutContext(Qt.ShortcutContext.WidgetShortcut)
        self.canvas.addAction(action)

    def add_menu_action(self, menu: QMenu, text: str, shortcut, callback) -> QAction:
        action = QAction(text, self)
        if shortcut is not None:
            action.setShortcut(shortcut)
        action.triggered.connect(callback)
        menu.addAction(action)
        return action

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
        from PySide6.QtWidgets import QMessageBox  # local import; avoids top-level cycle

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
        self.clear_selection()
        self.current_level = 0
        self.barrier_kind_colors = {}
        self.bridge_kind_colors = {}
        self.wall_width_cells = DEFAULT_WALL_WIDTH_CELLS
        self.forget_nested_map_shapes()
        self.refresh_ui()
        self.canvas.fit_map()
        self.review_repairs(quiet=True)

    def focus_issue(self, issue) -> None:
        if issue.level is not None:
            self.set_level_index(issue.level)
        self.canvas.issue_rects = [issue.rect] if issue.rect is not None else []
        if issue.rect is not None:
            self.canvas.viewport.focus(issue.rect, self.canvas.width(), self.canvas.height())
        self.canvas.update()

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
            self._flash_status(f"Recent file missing: {candidate}")
            # Drop the dead entry so the user doesn't keep tripping on it.
            remaining = [p for p in self._load_recent_paths() if p != path_str]
            self.preferences.setValue(self.RECENT_FILES_KEY, remaining)
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
        self.mode_combo.setToolTip("Select Tiles: click or drag tiles. Alt/Option edits spawn zones and nested-map ends.")
        tool_settings_action = toolbar.addWidget(self.tool_settings)
        self.tool_settings.available_changed.connect(tool_settings_action.setVisible)
        tool_settings_action.setVisible(False)
        # Persistent "Building UP/DOWN" hint that disambiguates the two ramp
        # modes mid-drag. Hidden outside ramp modes so it doesn't clutter the
        # toolbar.
        self.ramp_direction_label = QLabel()
        self.ramp_direction_label.setStyleSheet("color: #fbbf24; padding: 0 8px;")
        toolbar.addWidget(self.ramp_direction_label)
        toolbar.addAction(self.issues_action)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, toolbar)

    # === State updates & UI refresh ===

    def set_map(self, map_data: dict, mark_dirty: bool) -> None:
        self.doc.set_data(map_data, mark_dirty)

    def _on_document_changed(self, before: dict) -> None:
        self.cancel_interaction()
        self.canvas.issue_rects = []
        prior_selection: tuple[str, dict] | None = None
        if self.selected_spawn_zone_ref is not None:
            ref = self.selected_spawn_zone_ref
            if 0 <= ref.index < len(before[ref.list_name]):
                prior_selection = (ref.list_name, copy.deepcopy(before[ref.list_name][ref.index]))
        if self.tile_selection is not None:
            c0, r0, c1, r1 = self.tile_selection
            c1 = min(c1, self.map_data["grid_cols"])
            r1 = min(r1, self.map_data["grid_rows"])
            self.tile_selection = (c0, r0, c1, r1) if c0 < c1 and r0 < r1 else None
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        if prior_selection is not None:
            list_name, snapshot = prior_selection
            self.selected_spawn_zone_ref = self._zone_ref_after_change(list_name, snapshot)
        else:
            self.selected_spawn_zone_ref = None
        self.refresh_ui()

    def apply_change(self, label: str, after: dict) -> None:
        self.doc.apply_change(label, after)

    def refresh_ui(self) -> None:
        self.level_combo.blockSignals(True)
        self.level_combo.clear()
        for idx, level in enumerate(self.map_data["levels"]):
            self.level_combo.addItem(level_label(level, idx))
        self.level_combo.setCurrentIndex(self.current_level)
        self.level_combo.blockSignals(False)
        self.canvas.update()
        self.update_selection_actions()
        self.update_status()
        suffix = "*" if self.dirty else ""
        file_name = str(self.path) if self.path else "Untitled"
        self.setWindowTitle(f"Cuboid Wars Editor - {file_name}{suffix}")
        self.dependencies.watch(self.nested_map_shapes)
        self.tool_settings.refresh()

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
            self._flash_status(f"Catalog reload failed: {exc}")
        self.refresh_ui()

    def update_status(self, *, validate: bool = True) -> None:
        if validate:
            errors = validate_map(
                self.map_data,
                self.barrier_kinds,
                self.bridge_kinds,
                map_name=self.edited_map_name(),
                nested_lookup=self.nested_map_shape,
                actor_kinds=self.actor_kinds,
                material_aliases=self.materials_catalog,
            )
            self.issues_panel.set_issues(errors.issues)
            self.issues_action.setText(f"Issues ({len(errors)})")
            self.issues_action.setToolTip("\n".join(errors[:20]))
            self.issues_action.setVisible(bool(errors))
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

    def _flash_status(self, message: str) -> None:
        self.canvas.notice.show_message(message)

    # === Navigation (level / tool selection) ===

    def select_level(self, index: int) -> None:
        if 0 <= index < len(self.map_data["levels"]):
            self.cancel_interaction()
            self.current_level = index
            self.canvas.update()
            self.update_status(validate=False)

    def set_mode(self, mode: str) -> None:
        self.cancel_interaction()
        self.mode = mode
        self.canvas.setCursor(self._cursor_for_mode(mode))
        self.canvas.update()
        self.update_selection_actions()
        self.update_status(validate=False)
        self.tool_settings.refresh()

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
        self.level_combo.setCurrentIndex(clamped)

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
            self._clear_autosave()
            self.window_geometry.save()
            event.accept()
        else:
            event.ignore()

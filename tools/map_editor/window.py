"""Main map editor window."""

from __future__ import annotations

import copy
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction, QKeySequence, QShortcut, QUndoStack
from PySide6.QtWidgets import QComboBox, QLabel, QMainWindow, QMenu, QToolBar

from .canvas import Canvas
from .commands import SetMapCommand
from .constants import BARRIER_KIND_TABLE, DEFAULT_ACTOR_COUNT, MODE_FLOOR, MODES, STATUS_TIMEOUT_MS
from .erase import EraseMixin
from .file_actions import FileActionsMixin
from .geometry import canonicalize_map, level_label
from .io import empty_map, load_materials_catalog, read_map
from .lights import LightsMixin
from .placement import PlacementMixin
from .spawn_zones import SpawnZoneEditMixin
from .structure import StructureMixin
from .types import SpawnZoneDrag, ZoneRef
from .validation import validate_map


class EditorWindow(
    FileActionsMixin,
    PlacementMixin,
    LightsMixin,
    EraseMixin,
    StructureMixin,
    SpawnZoneEditMixin,
    QMainWindow,
):
    def __init__(self, path: Path):
        super().__init__()
        self.path: Path | None = path
        self.map_data = read_map(path) if path.exists() else empty_map()
        self.current_level = 0
        self.mode = MODE_FLOOR
        self.dirty = False
        self.undo_stack = QUndoStack(self)
        self.shortcuts = []
        # No default kind: the dialog opens with Kind blank on the first paint
        # of a session and remembers the last value across subsequent paints.
        self.recent_actor_spawn_kind: str = ""
        self.recent_actor_spawn_count: int = DEFAULT_ACTOR_COUNT
        # Last-used barrier kind, seeded from the first configured kind so the
        # picker dialog has a sensible default on the first paint.
        self.recent_barrier_kind: str | None = BARRIER_KIND_TABLE[0] if BARRIER_KIND_TABLE else None
        self.recent_key_kind: str | None = BARRIER_KIND_TABLE[0] if BARRIER_KIND_TABLE else None
        # (row_spacing, row_offset, col_spacing, col_offset) — remembered
        # across opens of the Auto-Place Lights dialog. Spacing is "cells
        # skipped between lights": 0 = every cell, 1 = every other, 2 = every
        # third.
        self.recent_auto_place_lights: tuple[int, int, int, int] = (0, 0, 0, 0)
        self.selected_spawn_zone_ref: ZoneRef | None = None
        self.spawn_zone_drag: SpawnZoneDrag | None = None
        self.show_material_overlay = False
        # Material used as the default for newly painted floors / walls / ramps.
        # The user picks a different one from the materials palette.
        self.current_material: str = "fiberous-plaster1-ue"
        self.materials_catalog: list[str] = load_materials_catalog(self.path)

        self.canvas = Canvas(self)
        self.setCentralWidget(self.canvas)
        self.setWindowTitle("Cuboid Wars Editor")

        self.level_combo = QComboBox()
        self.level_combo.currentIndexChanged.connect(self.select_level)
        self.mode_combo = QComboBox()
        self.mode_combo.addItems(MODES)
        self.mode_combo.currentTextChanged.connect(self.set_mode)
        self.status_label = QLabel()

        self.build_menus()
        self.build_toolbar()
        self.statusBar().addPermanentWidget(self.status_label)
        self.refresh_ui()
        self.resize_to_map()

    # === Menus & toolbar ===

    def build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&File")
        self.add_menu_action(file_menu, "&Open...", QKeySequence.StandardKey.Open, self.open_file)
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

    def build_toolbar(self) -> None:
        toolbar = QToolBar("Tools", self)
        toolbar.setMovable(False)
        toolbar.addWidget(QLabel("Level "))
        toolbar.addWidget(self.level_combo)
        toolbar.addSeparator()
        toolbar.addWidget(QLabel("Tool "))
        toolbar.addWidget(self.mode_combo)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, toolbar)

    # === State updates & UI refresh ===

    def set_map(self, map_data: dict, mark_dirty: bool) -> None:
        prior_selection: tuple[str, dict] | None = None
        if self.selected_spawn_zone_ref is not None:
            ref = self.selected_spawn_zone_ref
            if 0 <= ref.index < len(self.map_data[ref.list_name]):
                prior_selection = (ref.list_name, copy.deepcopy(self.map_data[ref.list_name][ref.index]))
        self.map_data = canonicalize_map(map_data)
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        if prior_selection is not None:
            list_name, snapshot = prior_selection
            self.selected_spawn_zone_ref = self._zone_ref_after_change(list_name, snapshot)
        else:
            self.selected_spawn_zone_ref = None
        if mark_dirty:
            self.dirty = True
        self.refresh_ui()

    def apply_change(self, label: str, after: dict) -> None:
        before = self.map_data
        if canonicalize_map(before) == canonicalize_map(after):
            return
        self.undo_stack.push(SetMapCommand(self, label, before, after))

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
        else:
            self.status_label.setText("Structurally valid")
            self.status_label.setToolTip("")

    def _flash_status(self, message: str) -> None:
        # Short-lived message in the bottom status bar — used for soft
        # rejections ("no walls in selection") and confirmations.
        self.statusBar().showMessage(message, STATUS_TIMEOUT_MS)

    # === Navigation (level / tool selection) ===

    def select_level(self, index: int) -> None:
        if 0 <= index < len(self.map_data["levels"]):
            self.current_level = index
            self.canvas.update()

    def set_mode(self, mode: str) -> None:
        self.mode = mode

    def set_material_overlay(self, enabled: bool) -> None:
        self.show_material_overlay = enabled
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
        self.set_tool_index(self.mode_combo.currentIndex() - 1)

    def next_tool(self) -> None:
        self.set_tool_index(self.mode_combo.currentIndex() + 1)

    def set_tool_index(self, index: int) -> None:
        count = self.mode_combo.count()
        if count == 0:
            return
        self.mode_combo.setCurrentIndex(index % count)

    # === Close handler ===

    def closeEvent(self, event) -> None:
        if self.confirm_discard_changes():
            event.accept()
        else:
            event.ignore()

"""Main map editor window."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QSettings, Qt, QTimer
from PySide6.QtGui import (
    QAction,
    QKeySequence,
    QShortcut,
    QUndoStack,
)
from PySide6.QtWidgets import QLabel, QMainWindow, QMenu, QToolBar

from .canvas import CLICK_TOOLS, Canvas
from .canvas_scroll import CanvasScrollArea
from .catalogs import (
    MapCatalogs,
    load_actor_kinds,
    load_wall_light_kinds,
    map_name_from_path,
    require_map_settings,
)
from .control_actions import ControlActionsMixin
from .constants import (
    DEFAULT_ACTOR_BEAM_IN_SECS,
    DEFAULT_ACTOR_COUNT,
    DEFAULT_ACTOR_RESPAWN_SECS,
    MODE_SELECT,
)
from .dependencies import MapDependencies
from .document import MapDocument
from .erase_tools import EraseMixin
from .file_actions import FileActionsMixin
from .issues import IssuesDialog
from .items import ItemsMixin
from .jump_reach_overlay import JumpReachOverlay
from .portal_jump_overlay import PortalJumpOverlay
from .ladders import LaddersMixin
from .lights import LightsMixin
from .nested_definitions import NestedDefinitionsMixin
from .nested_maps import NestedMapsMixin
from .nesting import NestedMotion
from .normalization import level_label
from .placement import PlacementMixin
from .run_time_overlay import RunTimeOverlay
from .selection_actions import SelectionActionsMixin
from .selection_state import SelectionMixin
from .selection import Selection
from .spawn_zones import SpawnZoneEditMixin
from .structure import StructureMixin
from .tool_settings import ToolSettings
from .workflow import WorkflowMixin
from .selection_transfer import SelectionTransferMixin
from .selection_properties import SelectionProperties
from .switch_connections import ConnectionOverlay
from .tool_catalog import MODE_TO_TOOL
from .tool_palette import ToolPalette
from .validation import (
    ValidationErrors,
    document_checkpoint_numbers,
    placed_definitions,
    plated_switches,
    validate_document,
    validate_map,
)
from .window_geometry import WindowGeometry
from .panel_layout import PanelLayout
from .compact_widgets import CompactComboBox


class EditorWindow(
    WorkflowMixin,
    SelectionTransferMixin,
    ControlActionsMixin,
    FileActionsMixin,
    NestedDefinitionsMixin,
    PlacementMixin,
    ItemsMixin,
    LightsMixin,
    LaddersMixin,
    NestedMapsMixin,
    EraseMixin,
    StructureMixin,
    SelectionMixin,
    SelectionActionsMixin,
    SpawnZoneEditMixin,
    QMainWindow,
):
    def __init__(self, path: Path, *, preferences: QSettings | None = None):
        super().__init__()
        map_name = map_name_from_path(path)
        require_map_settings(map_name)
        self.displayed_map = None
        self.preferences = preferences if preferences is not None else QSettings()
        # The document is the map being edited (data, file identity, dirty
        # state, undo history); the window holds view/tool state and widgets.
        self.doc = MapDocument(path)
        # Material for newly painted floors, walls, and ramps; an alias, since
        # face values are validated against the catalog on save.
        self.current_material = ""
        self.sampled_materials = None
        self.selection_levels = 1
        self.selection_kind = "Objects"
        self.clipboard_objects = False
        self.pending_block = None
        self.adopt_catalogs(map_name, MapCatalogs.load(map_name))
        self.actor_kinds = load_actor_kinds()
        self.wall_light_kinds = load_wall_light_kinds()
        self.recent_light_kind = next(iter(self.wall_light_kinds), "")
        self.recent_actor_spawn_levels = 1
        self.recent_actor_roam_distance = 0.0
        self.current_level = 0
        self.mode = MODE_SELECT
        self.shortcuts = []
        # The last values placed, shown in the toolbar and reused without a
        # prompt; the kinds start on the map's first listed kind.
        self.recent_barrier_controls = {}
        self.recent_bridge_controls = {}
        self.recent_checkpoint_number: int = 1
        self.recent_checkpoint_type: str = "individual"
        self.recent_actor_spawn_kind: str = ""
        self.recent_actor_spawn_count: list[int] = [DEFAULT_ACTOR_COUNT]
        self.recent_actor_spawn_respawn_secs: int | None = DEFAULT_ACTOR_RESPAWN_SECS
        self.recent_actor_beam_in_secs: float = DEFAULT_ACTOR_BEAM_IN_SECS
        # Empty = the zone has no switch.
        self.recent_actor_spawn_switch: str = ""
        self.recent_actor_spawn_inverted = False
        # None = the zone stays active whatever checkpoint the players reach.
        self.recent_actor_until_checkpoint: int | None = None
        self.recent_actor_on_checkpoint: str = "stop"
        first_kind = self.barrier_kinds[0] if self.barrier_kinds else None
        self.recent_barrier_kind: str | None = first_kind
        self.recent_pressure_plate_switch: str | None = self.switches[0] if self.switches else None
        self.recent_item_type: str = self.pickup_types[0]
        self.recent_item_key_kind: str | None = first_kind
        first_bridge_kind = self.bridge_kinds[0] if self.bridge_kinds else None
        self.recent_bridge_kind: str | None = first_bridge_kind
        # (row_spacing, row_offset, col_spacing, col_offset) — remembered
        # across opens of the Auto-Place Lights dialog. Spacing is "cells
        # skipped between lights": 0 = every cell, 1 = every other, 2 = every
        # third.
        self.recent_auto_place_lights: tuple[int, int, int, int] = (0, 0, 0, 0)
        self.recent_ladder_levels: int = 1
        self.recent_ramp_levels: int = 1
        self.recent_ramp_shape: str = "solid"
        # The side the last ramp rose toward; a click without a drag reuses it.
        self.recent_ramp_direction: str = "N"
        # The last nested map dialog answer:
        # (map, to_level, travel_secs, pause, phase, from_nudge, to_nudge).
        self.recent_nested_map: NestedMotion | None = None
        # `(level_idx, [light, ...])` while an Auto-Place Lights confirmation
        # is pending; canvas paints these as ghosts. `None` outside the
        # preview window.
        self.pending_auto_lights: tuple[int, list[dict]] | None = None
        self.selection = Selection()
        self.tile_clipboard: dict | None = None
        self.show_material_overlay = False
        self.show_roam_extensions = False
        # Show prev/next level geometry as ghosted overlays, and the ramps
        # arriving at this level from further below.
        self.show_adjacent_levels = False

        self.canvas = Canvas(self)
        self.canvas.setCursor(self.cursor_for_mode(self.mode))
        self.canvas_scroll = CanvasScrollArea(self.canvas)
        self.setCentralWidget(self.canvas_scroll)
        self.setWindowTitle("Cuboid Wars Editor")

        self.map_combo = CompactComboBox()
        self.map_combo.setAccessibleName("Map geometry")
        self.map_combo.currentIndexChanged.connect(self.select_map)
        self.level_combo = CompactComboBox()
        self.level_combo.currentIndexChanged.connect(self.select_level)
        self.tool_palette = ToolPalette(self)
        self.tool_palette.mode_requested.connect(self.activate_tool)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.tool_palette)
        self.issues_dialog = IssuesDialog(self)
        self.issues_dialog.focused.connect(self.focus_issue)
        self.issues_dialog.repair_requested.connect(self.review_repairs)
        self.properties_panel = SelectionProperties(self)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.properties_panel)
        self.connection_overlay = ConnectionOverlay(self)
        self.tool_palette.raise_()
        self.tool_settings = ToolSettings(self)

        self.jump_reach = JumpReachOverlay(self)
        self.run_time = RunTimeOverlay(self)
        self.portal_jump = PortalJumpOverlay(self)
        self.build_menus()
        self.build_toolbar()
        self.doc.changed.connect(self._on_document_changed)
        self.doc.saved.connect(self.refresh_ui)
        self.dependencies = MapDependencies(self)
        self.dependencies.changed.connect(self.reload_dependencies)
        self.refresh_ui()
        self.layout().activate()
        self.window_geometry = WindowGeometry(self, self.preferences)
        self.panel_layout = PanelLayout(self)
        self.canvas.fit_map()

        # Writes the recovery copy while the map is dirty, so a crash loses
        # at most the interval.
        self._autosave_timer = QTimer(self)
        self._autosave_timer.setInterval(self.AUTOSAVE_INTERVAL_MS)
        self._autosave_timer.timeout.connect(self._tick_autosave)
        self._autosave_timer.start()

    # === Document delegation ===

    @property
    def map_data(self) -> dict:
        return self.doc.map_data

    @property
    def barrier_kinds(self) -> list[str]:
        return list(self.barrier_kind_colors)

    @property
    def bridge_kinds(self) -> list[str]:
        return list(self.bridge_kind_colors)

    @property
    def key_kinds(self) -> list[str]:
        return self.barrier_kinds

    @property
    def switches(self) -> list[str]:
        return list(self.switch_ids)

    @property
    def dirty(self) -> bool:
        return self.doc.dirty

    @property
    def path(self) -> Path | None:
        return self.doc.path

    @property
    def undo_stack(self) -> QUndoStack:
        return self.doc.undo_stack

    # `plated_from` is the map whose plates operate the switches `data` names;
    # a clipboard block is checked against the map it is pasted into.
    def validate(self, data: dict, plated_from: dict | None = None) -> ValidationErrors:
        return validate_map(
            data,
            self.barrier_kinds,
            self.bridge_kinds,
            switches=self.switches,
            plated_switches=plated_switches(self._document_geometries(data if plated_from is None else plated_from)),
            map_name=self.doc.active_map,
            nested_lookup=self.nested_map_shape,
            actor_kinds=self.actor_kinds,
            wall_light_kinds=self.wall_light_kinds,
            material_aliases=self.materials_catalog,
            checkpoint_numbers=document_checkpoint_numbers([*self._document_geometries(data), data]),
        )

    # The root and every placed geometry of the document, with `data`
    # standing in for the active map.
    def _document_geometries(self, data: dict) -> list[dict]:
        active = self.doc.active_map
        root = data if active is None else self.doc.root_data
        definitions = {
            name: data if name == active else geometry for name, geometry in self.doc.nested_geometry.items()
        }
        return [root, *placed_definitions(root, definitions).values()]

    def document_issues(self) -> ValidationErrors:
        if self._document_issues is None:
            self._document_issues = self.validate_document(self.doc.root_data)
        return self._document_issues

    # The messages `after` adds to the current map's issues: an edit may not
    # introduce an error, while the issues already there stay Check Map's.
    def added_issues(self, after: dict) -> list[str]:
        if self._map_issues is None:
            self._map_issues = {issue.identity() for issue in self.validate(self.map_data).issues}
        current = self._map_issues
        return [issue.message for issue in self.validate(after).issues if issue.identity() not in current]

    def added_document_issues(self, root: dict) -> list[str]:
        current = {issue.identity() for issue in self.document_issues().issues}
        return [issue.message for issue in self.validate_document(root).issues if issue.identity() not in current]

    # The whole document against the catalogs of `map_name`, or the adopted ones.
    def validate_document(self, data: dict, map_name: str | None = None) -> ValidationErrors:
        catalogs = (self.current_catalogs() if map_name is None else MapCatalogs.load(map_name)).for_layout(data)
        return validate_document(
            data,
            catalogs,
            actor_kinds=self.actor_kinds,
            wall_light_kinds=self.wall_light_kinds,
        )

    def current_catalogs(self) -> MapCatalogs:
        return MapCatalogs(
            self.barrier_kind_colors,
            self.bridge_kind_colors,
            self.wall_width_cells,
            self.texture_catalog,
            self.switches,
            self.switch_colors,
            grid_cell_size=self.grid_cell_size,
            level_height=self.level_height,
            floor_thickness=self.floor_thickness,
            pickup_types=self.pickup_types,
        )

    # Every view, dialog, and validation reads the catalogs of one map;
    # opening, Save As, and a settings reload all switch them here.
    def adopt_catalogs(self, map_name: str, catalogs: MapCatalogs) -> None:
        self._map_issues = None
        self._document_issues = None
        catalogs = catalogs.for_layout(self.doc.root_data)
        self.catalog_map = map_name
        self.barrier_kind_colors = catalogs.barrier_kind_colors
        self.bridge_kind_colors = catalogs.bridge_kind_colors
        self.switch_ids = list(catalogs.switches)
        self.switch_colors = dict(catalogs.switch_colors)
        self.wall_width_cells = catalogs.wall_width_cells
        self.grid_cell_size = catalogs.grid_cell_size
        self.level_height = catalogs.level_height
        self.floor_thickness = catalogs.floor_thickness
        self.pickup_types = catalogs.pickup_types
        if hasattr(self, "recent_item_type") and self.recent_item_type not in self.pickup_types:
            self.recent_item_type = self.pickup_types[0]
        self.texture_catalog = catalogs.texture_catalog
        self.materials_catalog = list(catalogs.texture_catalog)
        if self.current_material not in catalogs.texture_catalog:
            self.current_material = next(iter(catalogs.texture_catalog), "")

    def adopt_map(self, map_name: str) -> None:
        self.adopt_catalogs(map_name, MapCatalogs.load(map_name))
        self.jump_reach.reload_settings()
        self.portal_jump.reload_settings()
        self.run_time.reload_settings()
        self.clear_selection()
        self.current_level = 0
        self.refresh_ui()
        self.canvas.fit_map()

    # === Menus & toolbar ===

    # Map each mode to the cursor it should display so a peripheral glance
    # tells the user which tool is active without reading the toolbar.
    def cursor_for_mode(self, mode: str) -> Qt.CursorShape:
        if mode == MODE_SELECT:
            return Qt.CursorShape.ArrowCursor
        if mode in CLICK_TOOLS:
            return Qt.CursorShape.PointingHandCursor
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
        self.add_menu_action(edit_menu, "Auto-Place &Lights...", None, self.open_auto_place_lights_dialog)
        self.add_menu_action(edit_menu, "&Clear Lights On Level", None, self.clear_lights_on_current_level)

        map_menu = self.menuBar().addMenu("&Map")
        self.add_menu_action(map_menu, "New Nested Map...", None, self.new_nested_map)
        self.rename_nested_action = self.add_menu_action(map_menu, "Rename Nested Map...", None, self.rename_nested_map)
        self.delete_nested_action = self.add_menu_action(map_menu, "Delete Nested Map", None, self.delete_nested_map)
        map_menu.addSeparator()
        self.add_menu_action(map_menu, "Resi&ze Map...", None, self.resize_map)
        self.add_menu_action(map_menu, "Edit &Levels…", None, self.edit_levels)
        self.add_menu_action(map_menu, "Edit &Checkpoints…", None, self.edit_checkpoints)
        map_menu.addSeparator()
        self.build_control_menu(map_menu)

        view_menu = self.menuBar().addMenu("&View")
        self.add_menu_action(view_menu, "Next Level", QKeySequence(Qt.Key.Key_PageUp), self.next_level)
        self.add_menu_action(view_menu, "Previous Level", QKeySequence(Qt.Key.Key_PageDown), self.previous_level)
        view_menu.addSeparator()
        zoom_in = self.add_menu_action(view_menu, "Zoom &In", None, lambda: self.canvas.zoom_by(1.25))
        zoom_in.setShortcuts(QKeySequence.StandardKey.ZoomIn)
        zoom_out = self.add_menu_action(view_menu, "Zoom &Out", None, lambda: self.canvas.zoom_by(0.8))
        zoom_out.setShortcuts(QKeySequence.StandardKey.ZoomOut)
        fit_action = self.add_menu_action(view_menu, "&Fit Map", QKeySequence("F"), self.canvas.fit_map)
        self.canvas_shortcut(fit_action)
        view_menu.addSeparator()
        self.tool_icons_action = QAction("Tool Icons Only", self)
        self.tool_icons_action.setCheckable(True)
        self.tool_icons_action.setChecked(self.tool_palette.icon_only)
        self.tool_icons_action.toggled.connect(self.tool_palette.set_icon_only)
        view_menu.addAction(self.tool_icons_action)
        self.connections_action = QAction("Show &Connections", self)
        self.connections_action.setCheckable(True)
        self.connections_action.toggled.connect(self.connection_overlay.set_enabled)
        view_menu.addAction(self.connections_action)
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
        self.roam_extensions_action = QAction("Show &Roam Extensions", self)
        self.roam_extensions_action.setCheckable(True)
        self.roam_extensions_action.setChecked(self.show_roam_extensions)
        self.roam_extensions_action.setShortcut(QKeySequence("R"))
        self.roam_extensions_action.toggled.connect(self.set_roam_extensions)
        view_menu.addAction(self.roam_extensions_action)
        self.canvas_shortcut(self.roam_extensions_action)
        view_menu.addAction(self.jump_reach.clear_action)
        view_menu.addAction(self.portal_jump.clear_action)
        view_menu.addAction(self.run_time.clear_action)

        help_menu = self.menuBar().addMenu("&Help")
        self.add_menu_action(help_menu, "Tool &Reference", None, self.show_tool_reference)

        self.add_shortcut(Qt.Key.Key_Left, self.previous_tool)
        self.add_shortcut(Qt.Key.Key_Right, self.next_tool)
        self.add_shortcut(Qt.Key.Key_E, self.tool_palette.toggle_erase)
        self.add_shortcut(Qt.Key.Key_I, self.sample_under_cursor)
        self.add_shortcut(Qt.Key.Key_Return, lambda: self.refresh_inspection(show=True))

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

    def createPopupMenu(self):
        return None

    def build_toolbar(self) -> None:
        toolbar = QToolBar("Tools", self)
        toolbar.setMovable(False)
        toolbar.addWidget(QLabel("Map "))
        toolbar.addWidget(self.map_combo)
        toolbar.addSeparator()
        toolbar.addWidget(QLabel("Level "))
        toolbar.addWidget(self.level_combo)
        toolbar.addSeparator()
        tool_settings_action = toolbar.addWidget(self.tool_settings)
        self.tool_settings.available_changed.connect(tool_settings_action.setVisible)
        tool_settings_action.setVisible(False)
        toolbar.addAction(self.jump_reach.controls_action)
        toolbar.addAction(self.portal_jump.controls_action)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, toolbar)
        self.addToolBarBreak(Qt.ToolBarArea.TopToolBarArea)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, self.jump_reach.toolbar)
        self.addToolBarBreak(Qt.ToolBarArea.TopToolBarArea)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, self.run_time.toolbar)
        self.addToolBarBreak(Qt.ToolBarArea.TopToolBarArea)
        self.addToolBar(Qt.ToolBarArea.TopToolBarArea, self.portal_jump.toolbar)

    # === State updates & UI refresh ===

    def _on_document_changed(self, before: dict) -> None:
        self._map_issues = None
        self._document_issues = None
        switched = self.displayed_map != self.doc.active_map
        if switched:
            self.clear_selection()
            self.current_level = 0
        self.displayed_map = self.doc.active_map
        self.selection = self.selection.refreshed(before, self.map_data)
        self.cancel_interaction()
        self.canvas.issue_rects = []
        self.current_level = max(0, min(self.current_level, len(self.map_data["levels"]) - 1))
        self.selection_levels = max(1, min(self.selection_levels, len(self.map_data["levels"]) - self.current_level))
        self.refresh_ui()
        if switched:
            self.canvas.fit_map()

    def apply_change(self, label: str, after: dict, merge_key: object | None = None) -> bool:
        return self.doc.apply_change(label, after, merge_key)

    def refresh_ui(self) -> None:
        self.adopt_catalogs(self.catalog_map, self.current_catalogs())
        self.map_combo.blockSignals(True)
        self.map_combo.clear()
        self.map_combo.addItem("Outer map", None)
        for name in sorted(self.doc.nested_geometry):
            self.map_combo.addItem(name, name)
        self.map_combo.setCurrentIndex(max(0, self.map_combo.findData(self.doc.active_map)))
        self.map_combo.blockSignals(False)
        self.rename_nested_action.setEnabled(self.doc.active_map is not None)
        self.delete_nested_action.setEnabled(self.doc.active_map is not None)
        self.level_combo.blockSignals(True)
        self.level_combo.clear()
        for idx, level in enumerate(self.map_data["levels"]):
            self.level_combo.addItem(level_label(level, idx))
        self.level_combo.setCurrentIndex(self.current_level)
        self.level_combo.blockSignals(False)
        self.canvas.refresh_view()
        self.update_selection_actions()
        self.refresh_issues()
        suffix = "*" if self.dirty else ""
        file_name = str(self.path) if self.path else "Untitled"
        self.setWindowTitle(f"Cuboid Wars Editor - {file_name}{suffix}")
        self.dependencies.watch(self.catalog_map)
        self.tool_settings.refresh()
        self.jump_reach.refresh()
        self.portal_jump.refresh()
        self.run_time.refresh()
        self.refresh_inspection()

    def show_map_issues(self):
        self.refresh_issues()
        self.issues_dialog.show()
        self.issues_dialog.raise_()
        self.issues_dialog.activateWindow()

    def refresh_issues(self) -> None:
        self.issues_dialog.set_issues(self.document_issues().issues)

    def notify(self, message: str) -> None:
        self.canvas.notice.show_message(message)

    def focus_issue(self, issue) -> None:
        self.doc.select_map(issue.map_name)
        if issue.level is not None:
            self.set_level_index(issue.level)
        self.canvas.issue_rects = [issue.rect] if issue.rect is not None else []
        if issue.rect is not None:
            self.canvas.viewport.focus(issue.rect, self.canvas.width(), self.canvas.height())
        self.canvas.refresh_view()

    # === Navigation (level / tool selection) ===

    def select_level(self, index: int) -> None:
        if 0 <= index < len(self.map_data["levels"]):
            self.cancel_interaction()
            self.current_level = index
            self.selection_levels = min(self.selection_levels, len(self.map_data["levels"]) - index)
            self.selection = Selection()
            self.refresh_inspection()
            self.update_selection_actions()
            self.tool_settings.refresh()
            self.canvas.update()

    def focus_canvas(self) -> None:
        self.activateWindow()
        self.canvas.setFocus()

    def activate_tool(self, mode: str) -> None:
        self.set_mode(mode)
        self.focus_canvas()

    def set_mode(self, mode: str) -> None:
        if mode not in MODE_TO_TOOL:
            raise ValueError(f"Unknown editor tool: {mode}")
        self.cancel_interaction()
        self.mode = mode
        self.selection = Selection()
        self.refresh_inspection()
        self.tool_palette.set_mode(mode)
        self.canvas.setCursor(self.cursor_for_mode(mode))
        self.canvas.update()
        self.update_selection_actions()
        self.tool_settings.refresh()
        self.jump_reach.refresh()
        self.portal_jump.refresh()
        self.run_time.refresh()

    def set_roam_extensions(self, enabled: bool) -> None:
        self.show_roam_extensions = enabled
        self.canvas.update()

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
        self.tool_palette.step(direction)

    # === Close handler ===

    def closeEvent(self, event) -> None:
        if self.confirm_discard_changes():
            self._clear_autosave()
            self.window_geometry.save()
            event.accept()
        else:
            event.ignore()

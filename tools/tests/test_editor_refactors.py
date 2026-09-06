import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from PySide6.QtCore import QEvent, QPoint, QPointF, QSettings, QTimer, Qt
from PySide6.QtGui import QContextMenuEvent, QMouseEvent, QWheelEvent
from PySide6.QtTest import QSignalSpy, QTest
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QDockWidget,
    QMenu,
    QMessageBox,
    QStatusBar,
    QToolBar,
)

from map_editor.constants import (
    DEFAULT_ALIAS,
    FACES,
    MODE_ACTOR_SPAWN_ZONE,
    MODE_BRIDGE_PLATE,
    MODE_ERASE,
    MODE_FIREWORK_PLATE,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_NESTED_MAP,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_UP,
    MODE_SELECT,
    MODE_WALL,
    MODES,
)
from map_editor.canvas import CLICK_TOOLS, RELEASE_TOOLS
from map_editor.dependencies import MapDependencies
from map_editor.dialogs import ActorSpawnFieldsDialog, MaterialAssignmentDialog
from map_editor.document import MapDocument
from map_editor.editing import material_values, paint_floors, place_plate, top_left_materials, update_records
from map_editor.erasing import erase_cell_rect
from map_editor.io import empty_map, read_map, write_map
from map_editor.normalization import normalize_map, pressure_plate_key
from map_editor.structure import insert_level_data, remove_level_data
from map_editor.transforms import record_lists, resize_map_data, translate_map
from map_editor.validation import validate_map
from map_editor.viewport import Viewport
from map_editor.window import EditorWindow


class DocumentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        self.path = self.directory / "map.json"
        write_map(self.path, empty_map(8, 8))
        self.doc = MapDocument(self.path, recovery_dir=self.directory / "recovery")

    def tearDown(self):
        self.doc.clear_autosave()
        self.temp.cleanup()

    def test_document_owns_transactions_and_signals_without_a_window(self):
        before = copy.deepcopy(self.doc.map_data)
        changed = QSignalSpy(self.doc.changed)
        after = paint_floors(before, 0, (2, 2, 3, 3), DEFAULT_ALIAS)
        self.assertEqual(self.doc.map_data, before)
        self.assertTrue(self.doc.apply_change("Paint", after))
        self.assertTrue(self.doc.dirty)
        self.assertEqual(changed.count(), 1)
        self.assertEqual(changed.at(0)[0], before)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)
        self.assertFalse(self.doc.dirty)
        self.doc.undo_stack.redo()
        self.assertTrue(self.doc.dirty)
        self.assertFalse(self.doc.apply_change("No change", self.doc.map_data))
        self.assertEqual(self.doc.undo_stack.count(), 1)

    def load_damaged_map(self):
        data = empty_map(8, 8)
        data["levels"][0]["lights"] = [{"col": 2, "row": 2, "side": "invalid"}]
        data["ladders"] = [{"col": 3, "row": 3, "lower_level": 0, "levels": 0, "side": "invalid"}]
        data["items"] = [{"col": 7, "row": 7, "level": 0, "type": "cookie"}]
        data["actor_spawn_zones"] = [{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "unknown", "count": -2}]
        write_map(self.path, data)
        self.doc.load(self.path)
        return self.doc.map_data

    def test_loading_preserves_invalid_records_and_reports_them(self):
        data = self.load_damaged_map()
        self.assertEqual(len(data["items"]), 1)
        self.assertEqual(data["ladders"][0]["levels"], 0)
        self.assertEqual(data["levels"][0]["lights"][0]["side"], "INVALID")
        errors = validate_map(data, [], [], actor_kinds=["beetle"])
        self.assertTrue(any("unknown actor kind" in error for error in errors))
        self.assertTrue(any("negative count" in error for error in errors))
        self.assertFalse(self.doc.dirty)

    def test_explicit_repairs_are_one_undoable_edit(self):
        before = copy.deepcopy(self.load_damaged_map())
        repaired, summary = self.doc.proposed_repairs()
        self.assertTrue(any("lights" in line for line in summary))
        self.assertTrue(any("items" in line for line in summary))
        self.doc.apply_change("Repair Map", repaired, repair=True)
        self.assertEqual(self.doc.map_data["levels"][0]["lights"], [])
        self.assertTrue(self.doc.dirty)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)
        self.assertFalse(self.doc.dirty)

    def test_unrelated_edits_and_coordinate_transforms_do_not_repair_records(self):
        self.load_damaged_map()
        self.doc.apply_change("Paint", paint_floors(self.doc.map_data, 0, (4, 4, 5, 5), DEFAULT_ALIAS))
        self.assertEqual(len(self.doc.map_data["items"]), 1)
        moved = resize_map_data(self.doc.map_data, 10, 10, 2, 2)
        self.doc.apply_change("Resize", moved)
        self.assertEqual(self.doc.map_data["levels"][0]["lights"][0]["col"], 4)
        self.doc.apply_change("Insert", insert_level_data(self.doc.map_data, 0))
        self.assertEqual(self.doc.map_data["levels"][1]["lights"][0]["side"], "INVALID")
        self.assertEqual(self.doc.map_data["items"][0]["level"], 1)

    def test_untitled_recovery_preserves_data_and_rejects_an_active_session(self):
        self.doc.replace_with_new(empty_map())
        self.doc.write_autosave()
        recovery = self.doc.autosave_path()
        self.assertTrue(recovery.exists())
        other = MapDocument(None, recovery_dir=self.directory / "other")
        self.assertFalse(other.recover_session(recovery))
        self.doc.recovery_lock.unlock()
        self.doc.recovery_lock = None
        self.assertTrue(other.recover_session(recovery))
        self.assertEqual(other.map_data, self.doc.map_data)
        self.assertIsNone(other.path)
        self.assertTrue(other.dirty)
        destination = self.directory / "recovered.json"
        other.write(destination)
        self.assertFalse(recovery.exists())
        self.assertEqual(read_map(destination), other.map_data)

    def test_failed_recovery_leaves_current_document_untouched(self):
        before = copy.deepcopy(self.doc.map_data)
        with self.assertRaises(FileNotFoundError):
            self.doc.recover_session(self.directory / "missing.json")
        self.assertEqual(self.doc.map_data, before)
        self.assertEqual(self.doc.path, self.path)

    def test_inserting_through_ramps_requires_removal_and_undo_restores_everything(self):
        data = empty_map()
        data["levels"].append(copy.deepcopy(data["levels"][0]))
        data["ramps"] = [{"lower_level": 0, "low": [2, 2], "high": [5, 3], "all": DEFAULT_ALIAS}]
        data["ladders"] = [{"lower_level": 0, "levels": 1, "col": 6, "row": 6, "side": "N"}]
        self.doc.replace_with_new(data)
        before = copy.deepcopy(self.doc.map_data)
        with self.assertRaises(ValueError):
            insert_level_data(before, 1)
        after = insert_level_data(before, 1, remove_crossing_ramps=True)
        self.assertEqual(after["ramps"], [])
        self.assertEqual(after["ladders"][0]["levels"], 2)
        self.doc.apply_change("Insert", after)
        self.assertEqual(len(self.doc.map_data["levels"]), 3)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)

    def test_transform_and_erase_helpers_leave_their_input_unchanged(self):
        data = normalize_map(empty_map())
        data = paint_floors(data, 0, (2, 2, 3, 3), DEFAULT_ALIAS)
        before = copy.deepcopy(data)
        moved = translate_map(data, 2, 3)
        erased = erase_cell_rect(data, 0, (2, 2), (2, 2), False)
        self.assertEqual(data, before)
        self.assertEqual(moved["levels"][0]["floors"][0]["col"], 4)
        self.assertEqual(erased["levels"][0]["floors"], [])
        partial = {"levels": [{}]}
        list(record_lists(partial))
        self.assertEqual(partial, {"levels": [{}]})
        with self.assertRaises(ValueError):
            remove_level_data(data, 0)

    def test_material_validation_uses_the_supplied_catalog(self):
        data = paint_floors(empty_map(), 0, (3, 3, 4, 4), "fresh_alias")
        self.assertFalse(validate_map(data, [], [], material_aliases=["fresh_alias"]))
        self.assertTrue(validate_map(data, [], [], material_aliases=[DEFAULT_ALIAS]))

    def test_material_source_uses_spatial_order_not_record_or_wall_endpoint_order(self):
        pattern = dict(zip(FACES, ("a", "b", "c", "d", "e", "f")))
        cases = {
            "floors": ({"col": 2, "row": 1}, {"col": 1, "row": 2}),
            "walls": ({"c0": 3, "r0": 1, "c1": 2, "r1": 1}, {"c0": 0, "r0": 2, "c1": 1, "r1": 2}),
            "ramps": ({"low": [2, 1], "high": [5, 2]}, {"low": [0, 3], "high": [3, 4]}),
        }
        for name, (first, second) in cases.items():
            with self.subTest(name=name):
                entries = [{**second, **dict.fromkeys(FACES, "other")}, {**first, **pattern}]
                before = copy.deepcopy(entries)
                self.assertEqual(top_left_materials(entries, name), pattern)
                self.assertEqual(entries, before)


class WindowRefactorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "map.json"
        data = paint_floors(empty_map(8, 8), 0, (1, 1, 2, 2), DEFAULT_ALIAS)
        data["player_spawn_zones"] = []
        write_map(self.path, data)
        self.recents = patch.object(EditorWindow, "_record_recent_path")
        self.recents.start()
        self.app.clipboard().clear()
        preferences = QSettings(str(Path(self.temp.name) / "preferences.ini"), QSettings.Format.IniFormat)
        self.window = EditorWindow(self.path, preferences=preferences)
        self.window._autosave_timer.stop()
        self.window.show()
        self.window.activateWindow()
        self.app.processEvents()

    def tearDown(self):
        with patch.object(self.window, "confirm_discard_changes", return_value=True):
            self.window.close()
        self.window.deleteLater()
        self.app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
        self.recents.stop()
        self.temp.cleanup()

    def test_canvas_fills_window_below_one_toolbar_without_permanent_panels(self):
        window = self.window
        self.assertIsNone(window.findChild(QStatusBar))
        self.assertEqual(len(window.findChildren(QToolBar)), 1)
        self.assertFalse(any(dock.isVisible() for dock in window.findChildren(QDockWidget)))
        self.assertEqual(window.canvas.width(), window.contentsRect().width())
        self.assertEqual(window.canvas.geometry().bottom(), window.contentsRect().bottom())
        self.assertFalse(window.tool_settings.isVisible())

    def test_close_saves_geometry_shared_with_other_maps_but_cancel_keeps_window_open(self):
        window = self.window
        window.resize(600, 450)
        window.move(60, 70)
        self.app.processEvents()
        normal = window.geometry()
        with (
            patch.object(window, "confirm_discard_changes", return_value=False),
            patch.object(window.window_geometry, "save") as save,
        ):
            self.assertFalse(window.close())
            self.assertTrue(window.isVisible())
            save.assert_not_called()
        with patch.object(window, "confirm_discard_changes", return_value=True):
            self.assertTrue(window.close())
        self.assertFalse(window.window_geometry.timer.isActive())
        self.assertEqual(window.preferences.value(window.window_geometry.KEY), window.saveGeometry())
        other_path = self.path.with_name("other.json")
        write_map(other_path, empty_map(12, 16))
        self.window = EditorWindow(other_path, preferences=window.preferences)
        window.deleteLater()
        self.window._autosave_timer.stop()
        self.window.show()
        self.app.processEvents()
        self.assertEqual(self.window.geometry(), normal)

    def test_map_operations_fit_canvas_without_changing_window_geometry(self):
        window = self.window
        window.resize(600, 450)
        window.move(60, 70)
        for maximized in (False, True):
            with self.subTest(maximized=maximized):
                if maximized:
                    window.showMaximized()
                self.app.processEvents()
                geometry = window.geometry()
                normal = window.normalGeometry()
                with (
                    patch("map_editor.structure.ResizeMapDialog.prompt", return_value=(12, 12, 0, 0)),
                    patch.object(window, "confirm_discard_changes", return_value=True),
                    patch("map_editor.file_actions.QMessageBox.warning"),
                ):
                    for action in (window.resize_map, window.new_file, lambda: window.load_path(self.path)):
                        window.canvas.zoom_by(2)
                        action()
                        self.app.processEvents()
                        self.assertTrue(window.canvas.viewport.fitted)
                        self.assertEqual(window.geometry(), geometry)
                        self.assertEqual(window.normalGeometry(), normal)
                        self.assertEqual(window.isMaximized(), maximized)

    def move_with_button(self, canvas, position):
        self.app.sendEvent(canvas, QMouseEvent(
            QEvent.Type.MouseMove, QPointF(position), QPointF(canvas.mapToGlobal(position)),
            Qt.MouseButton.NoButton, Qt.MouseButton.LeftButton, Qt.KeyboardModifier.NoModifier,
        ))

    def test_single_tile_tools_preview_and_commit_one_target_without_drag_ranges(self):
        window = self.window
        canvas = window.canvas
        methods = {
            MODE_LADDER: "toggle_ladder_at",
            MODE_LIGHT: "add_light_at",
            MODE_PRESSURE_PLATE: "prompt_and_add_pressure_plate",
            MODE_BRIDGE_PLATE: "prompt_and_add_bridge_plate",
            MODE_FIREWORK_PLATE: "add_firework_plate",
            MODE_ITEM: "prompt_and_add_item",
        }
        self.assertEqual(set(CLICK_TOOLS), set(methods))
        self.assertFalse(set(CLICK_TOOLS) & set(RELEASE_TOOLS))
        self.assertEqual(set(CLICK_TOOLS) | set(RELEASE_TOOLS) | {MODE_SELECT}, set(MODES))
        for mode, method in methods.items():
            with self.subTest(mode=mode), patch.object(window, method) as place:
                window.mode_combo.setCurrentText(mode)
                self.app.processEvents()
                start = canvas.viewport.from_grid(QPointF(1.5, 1.1)).toPoint()
                end = canvas.viewport.from_grid(QPointF(4.5, 4.1)).toPoint()
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
                self.move_with_button(canvas, end)
                self.assertIsNone(canvas.drag_start_cell)
                self.assertIsNone(canvas.drag_current_cell)
                self.assertIsNone(canvas.drag_start_point)
                self.assertIsNone(canvas.drag_current_point)
                self.assertEqual(canvas.hover_cell, (4, 4))
                if mode in (MODE_LADDER, MODE_LIGHT):
                    self.assertEqual(canvas.hover_edge_side, "N")
                painter = Mock()
                canvas._paint_drag_preview_rect(painter, canvas.cell_size())
                painter.drawRect.assert_not_called()
                place.assert_not_called()
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
                if mode in (MODE_LADDER, MODE_LIGHT):
                    place.assert_called_once_with(canvas.map_position(end), canvas.cell_size())
                else:
                    place.assert_called_once_with(4, 4)
                self.assertFalse(canvas.click_pending)

    def test_single_tile_placement_cancels_on_escape_tool_change_or_off_grid_release(self):
        window = self.window
        canvas = window.canvas
        for cancel in ("escape", "tool", "outside"):
            with self.subTest(cancel=cancel), patch.object(window, "add_firework_plate") as place:
                window.mode_combo.setCurrentText(MODE_FIREWORK_PLATE)
                self.app.processEvents()
                position = canvas.viewport.from_grid(QPointF(2.5, 2.5)).toPoint()
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=position)
                if cancel == "escape":
                    QTest.keyClick(canvas, Qt.Key.Key_Escape)
                elif cancel == "tool":
                    window.mode_combo.setCurrentText(MODE_SELECT)
                else:
                    position = QPoint(-20, -20)
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=position)
                place.assert_not_called()
                self.assertFalse(canvas.click_pending)
        self.assertIsNone(window.tile_selection)

    def test_range_tools_still_receive_both_drag_endpoints(self):
        window = self.window
        canvas = window.canvas
        methods = {
            MODE_FLOOR: "add_floor_rect",
            MODE_ERASE: "erase_cell_rect",
            MODE_FLOOR_MATERIAL: "assign_floor_materials_rect",
            MODE_WALL: "add_wall_line",
            MODE_RAMP_UP: "add_ramp",
            MODE_NESTED_MAP: "drag_nested_map",
        }
        for mode, method in methods.items():
            with self.subTest(mode=mode), patch.object(window, method) as place:
                window.mode_combo.setCurrentText(mode)
                self.app.processEvents()
                start = canvas.viewport.from_grid(QPointF(1.1, 1.1)).toPoint()
                end = canvas.viewport.from_grid(QPointF(4.1, 1.1)).toPoint()
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
                self.move_with_button(canvas, end)
                self.assertEqual(canvas.drag_start_cell, (1, 1))
                self.assertEqual(canvas.drag_current_cell, (4, 1))
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
                place.assert_called_once()
                self.assertEqual(place.call_args.args[:2], ((1, 1), (4, 1)))

    def test_tool_settings_are_inline_and_hide_for_tools_without_properties(self):
        window = self.window
        toolbar = window.findChild(QToolBar)
        window.mode_combo.setCurrentText(MODE_ACTOR_SPAWN_ZONE)
        self.app.processEvents()
        self.assertTrue(window.tool_settings.isVisible())
        self.assertIs(window.tool_settings.parentWidget(), toolbar)
        self.assertLessEqual(window.tool_settings.height(), toolbar.height())
        self.assertLess(window.tool_settings.geometry().right(), toolbar.width())
        for mode in (MODE_SELECT, MODE_ERASE):
            window.mode_combo.setCurrentText(mode)
            self.app.processEvents()
            self.assertFalse(window.tool_settings.isVisible())
        window.mode_combo.setCurrentText(MODE_FLOOR)
        self.app.processEvents()
        self.assertTrue(window.tool_settings.isVisible())
        self.assertIsNone(window.tool_settings.body.findChild(QCheckBox))

    def test_item_kind_control_only_shows_for_keys_including_recalled_settings(self):
        window = self.window
        window.recent_item_type = "cookie"
        window.mode_combo.setCurrentText(MODE_ITEM)
        self.app.processEvents()
        item, kind = window.tool_settings.body.findChildren(QComboBox)
        self.assertFalse(kind.isVisible())
        item.setCurrentText("key")
        self.app.processEvents()
        self.assertTrue(kind.isVisible())
        window.recent_item_type = "cookie"
        window.tool_settings.refresh()
        self.app.processEvents()
        self.assertFalse(window.tool_settings.body.findChildren(QComboBox)[1].isVisible())

    def test_canvas_notice_replaces_and_expires_without_resizing_or_taking_focus(self):
        window = self.window
        canvas = window.canvas
        canvas.setFocus()
        geometry = canvas.geometry()
        window._flash_status("No wall here")
        notice = canvas.notice
        self.assertTrue(notice.isVisible())
        self.assertTrue(notice.testAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents))
        self.assertTrue(canvas.hasFocus())
        window._flash_status("Nothing to erase")
        self.assertEqual(notice.text(), "Nothing to erase")
        self.assertEqual(canvas.geometry(), geometry)
        self.assertIsNone(window.findChild(QStatusBar))
        notice.timer.start(10)
        QTest.qWait(30)
        self.assertFalse(notice.isVisible())
        self.assertEqual(canvas.geometry(), geometry)

    def test_long_canvas_notice_stays_inside_canvas_after_resize(self):
        window = self.window
        window._flash_status("Cannot place the item here. " * 12)
        window.resize(650, 500)
        self.app.processEvents()
        notice = window.canvas.notice
        self.assertTrue(window.canvas.rect().contains(notice.geometry()))
        self.assertGreater(notice.height(), notice.fontMetrics().height() * 2)
        self.assertEqual(notice.text(), "Cannot place the item here. " * 12)

    def test_toolbar_issues_action_only_appears_for_errors_and_opens_the_panel(self):
        window = self.window
        self.assertFalse(window.issues_action.isVisible())
        data = paint_floors(window.map_data, 0, (6, 5, 7, 6), "not_an_alias")
        window.apply_change("Invalid material", data)
        self.assertTrue(window.issues_action.isVisible())
        self.assertIn(str(window.issues_panel.list.count()), window.issues_action.text())
        self.assertFalse(window.issues_panel.isVisible())
        window.issues_action.trigger()
        self.assertTrue(window.issues_panel.isVisible())
        window.undo_stack.undo()
        self.assertFalse(window.issues_action.isVisible())

    def test_zoom_keeps_the_grid_point_under_the_mouse_and_fit_reaches_large_maps(self):
        viewport = Viewport()
        viewport.fit(600, 400, 256, 256)
        self.assertLessEqual(viewport.from_grid(QPointF(256, 256)).y(), 400)
        anchor = QPointF(125, 78)
        grid = viewport.to_grid(anchor)
        viewport.zoom(2, anchor)
        self.assertEqual(viewport.to_grid(anchor), grid)
        viewport.pan(QPointF(-50, 20))
        point = QPointF(3.5, 4.5)
        self.assertEqual(viewport.to_grid(viewport.from_grid(point)), point)

    def test_wheel_zoom_and_panned_selection_use_the_same_transform(self):
        canvas = self.window.canvas
        anchor = QPointF(100, 100)
        grid = canvas.viewport.to_grid(anchor)
        event = QWheelEvent(
            anchor,
            anchor,
            QPoint(),
            QPoint(0, 120),
            Qt.MouseButton.NoButton,
            Qt.KeyboardModifier.NoModifier,
            Qt.ScrollPhase.ScrollUpdate,
            False,
        )
        self.app.sendEvent(canvas, event)
        self.assertAlmostEqual(canvas.viewport.to_grid(anchor).x(), grid.x())
        canvas.viewport.pan(QPointF(80, 60))
        position = canvas.viewport.from_grid(QPointF(1.5, 1.5)).toPoint()
        self.assertEqual(canvas.point_to_cell(position), (1, 1))
        QTest.mouseClick(canvas, Qt.MouseButton.LeftButton, pos=position)
        self.assertEqual(self.window.tile_selection, (1, 1, 2, 2))

    def test_middle_and_space_drag_pan_without_erasing(self):
        window = self.window
        window.mode_combo.setCurrentText(MODE_ERASE)
        canvas = window.canvas
        canvas.setFocus()
        before = copy.deepcopy(window.map_data)
        start, end = QPoint(100, 100), QPoint(160, 150)
        for button, space in ((Qt.MouseButton.MiddleButton, False), (Qt.MouseButton.LeftButton, True)):
            origin = QPointF(canvas.viewport.offset)
            if space:
                QTest.keyPress(canvas, Qt.Key.Key_Space)
            QTest.mousePress(canvas, button, pos=start)
            QTest.mouseMove(canvas, end)
            QTest.mouseRelease(canvas, button, pos=end)
            if space:
                QTest.keyRelease(canvas, Qt.Key.Key_Space)
            self.assertEqual(canvas.viewport.offset, origin + QPointF(60, 50))
        self.assertEqual(window.map_data, before)
        self.assertFalse(window.dirty)

    def test_panned_wall_and_light_hit_testing(self):
        window = self.window
        window.add_wall_line((1, 1), (2, 1))
        data = copy.deepcopy(window.map_data)
        data["levels"][0]["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        window.apply_change("Light", data)
        canvas = window.canvas
        canvas.viewport.pan(QPointF(60, 40))
        position = canvas.viewport.from_grid(QPointF(1.5, 1.15))
        hit = window.hit_at(canvas.map_position(position), canvas.cell_size())
        self.assertEqual(hit[0], "Light")
        wall = canvas._wall_near_position(canvas.viewport.from_grid(QPointF(1.5, 1)))
        self.assertIsNotNone(wall)

    def test_visible_entries_cull_offscreen_geometry(self):
        canvas = self.window.canvas
        canvas.viewport.cell = 100
        canvas.viewport.offset = QPointF(-1000, -1000)
        entries = [{"col": 1, "row": 1}, {"col": 12, "row": 12}]
        self.assertEqual(list(canvas.visible_entries("floors", entries)), entries[1:])

    def test_same_type_plates_can_be_edited_and_erased_independently(self):
        window = self.window
        window.barrier_kind_colors = {"a": "#ff0000", "b": "#00ff00", "c": "#0000ff"}
        window.add_pressure_plate(1, 1, "a")
        window.add_pressure_plate(1, 1, "b")
        a, b = window.plates_at(1, 1)
        with patch("map_editor.placement.KindDialog.prompt", return_value="c"):
            window.edit_pressure_plate_at(pressure_plate_key(a))
        self.assertEqual({p["kind"] for p in window.plates_at(1, 1)}, {"b", "c"})
        window.erase_pressure_plate(pressure_plate_key(b))
        self.assertEqual([p["kind"] for p in window.plates_at(1, 1)], ["c"])
        window.undo_stack.undo()
        self.assertEqual({p["kind"] for p in window.plates_at(1, 1)}, {"b", "c"})
        with self.assertRaises(ValueError):
            place_plate(window.map_data, b)

    def test_plate_context_actions_retain_their_purpose_keys(self):
        window = self.window
        window.barrier_kind_colors = {"a": "#ff0000", "b": "#00ff00"}
        window.add_pressure_plate(1, 1, "a")
        window.add_pressure_plate(1, 1, "b")
        canvas = window.canvas
        position = canvas.viewport.from_grid(QPointF(1.5, 1.5)).toPoint()

        def choose_erase(menu, *_):
            action = next(a for a in menu.actions() if a.text() == "Erase Barrier Plate (a)")
            action.trigger()

        menu = QMenu(canvas)
        menu.exec = lambda *_: choose_erase(menu)
        with patch("map_editor.canvas.QMenu", return_value=menu):
            canvas.contextMenuEvent(
                QContextMenuEvent(QContextMenuEvent.Reason.Mouse, position, canvas.mapToGlobal(position))
            )
        self.assertEqual([p["kind"] for p in window.plates_at(1, 1)], ["b"])

    def test_invalid_ladders_remain_erasable_without_breaking_other_tools(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["ladders"] = [{"lower_level": 0, "col": 2, "row": 2, "levels": 0, "side": "bad"}]
        window.doc.replace_with_new(data)
        hit = window.hit_at(QPointF(2.5, 2.5) * window.canvas.cell_size(), window.canvas.cell_size())
        self.assertEqual(hit[0], "Ladder")
        window.erase_ladders_rect((5, 5), (5, 5))
        self.assertEqual(len(window.map_data["ladders"]), 1)
        window.erase_ladders_rect((2, 2), (2, 2))
        self.assertEqual(window.map_data["ladders"], [])
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["ladders"]), 1)

    def test_mixed_materials_leave_untouched_faces_distinct(self):
        window = self.window
        first, second = window.materials_catalog[:2]
        data = paint_floors(window.map_data, 0, (2, 1, 3, 2), second)
        data = update_records(data, "floors", lambda f: f["col"] == 1, {"top": first}, 0)
        window.apply_change("Materials", data)
        floors = window.map_data["levels"][0]["floors"]
        initial = material_values(floors)
        self.assertIsNone(initial["top"])
        dialog = MaterialAssignmentDialog(window, "Materials", "2 tiles", window.materials_catalog, initial)
        self.assertNotIn("top", dialog.values())
        dialog._dropdowns["north"].setCurrentText(first)
        with patch("map_editor.placement.MaterialAssignmentDialog.prompt", return_value=dialog.values()):
            window.assign_floor_materials_rect((1, 1), (2, 1))
        self.assertEqual([f["top"] for f in window.map_data["levels"][0]["floors"]], [first, second])
        self.assertTrue(all(f["north"] == first for f in window.map_data["levels"][0]["floors"]))
        dialog.deleteLater()

    def test_material_source_button_fills_each_face_without_changing_the_map(self):
        window = self.window
        before = copy.deepcopy(window.map_data)
        pattern = dict(zip(FACES, window.materials_catalog[:6]))
        dialog = MaterialAssignmentDialog(window, "Materials", "2 tiles", window.materials_catalog, {}, source=pattern)
        self.assertEqual(dialog.values(), {})
        dialog.source_button.click()
        self.assertEqual(dialog.values(), pattern)
        dialog.reject()
        self.assertEqual(window.map_data, before)
        self.assertEqual(dialog.result(), QDialog.DialogCode.Rejected)
        dialog.deleteLater()

    def test_top_left_material_pattern_applies_to_selected_floors_and_walls_and_undoes(self):
        window = self.window
        pattern = dict(zip(FACES, window.materials_catalog[:6]))

        def use_source(*args, **kwargs):
            self.assertEqual(kwargs["source"], pattern)
            self.assertIsNone(args[4]["bottom"])
            dialog = MaterialAssignmentDialog(*args, **kwargs)
            dialog.source_button.click()
            values = dialog.values()
            dialog.deleteLater()
            return values

        for walls in (False, True):
            with self.subTest(walls=walls):
                data = empty_map(8, 8)
                if walls:
                    data["levels"][0]["walls"] = [
                        {"c0": 3, "r0": 3, "c1": 4, "r1": 3, "all": DEFAULT_ALIAS},
                        {"c0": 2, "r0": 2, "c1": 1, "r1": 2, **pattern},
                    ]
                else:
                    data["levels"][0]["floors"] = [{"col": 3, "row": 3, "all": DEFAULT_ALIAS}]
                    data["levels"][0]["inaccessible_floors"] = [{"col": 2, "row": 2, **pattern}]
                window.doc.replace_with_new(data)
                before = copy.deepcopy(window.map_data)
                with patch("map_editor.placement.MaterialAssignmentDialog.prompt", side_effect=use_source):
                    if walls:
                        window.assign_wall_materials_rect((0, 0), (5, 5))
                    else:
                        window.assign_floor_materials_rect((0, 0), (5, 5))
                level = window.map_data["levels"][0]
                entries = level["walls"] if walls else level["floors"] + level["inaccessible_floors"]
                self.assertTrue(all({face: entry[face] for face in FACES} == pattern for entry in entries))
                window.undo_stack.undo()
                self.assertEqual(window.map_data, before)

    def test_ladders_place_with_the_previous_span_without_a_dialog(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] *= 4
        window.doc.replace_with_new(data)
        window.recent_ladder_levels = 2
        with patch.object(QDialog, "exec", side_effect=AssertionError("Unexpected placement dialog")):
            for col in (3, 5):
                window.toggle_ladder_at(QPointF(col + 0.5, 3.05) * window.canvas.cell_size(), window.canvas.cell_size())
        self.assertEqual([ladder["levels"] for ladder in window.map_data["ladders"]], [2, 2])

    def test_item_and_kind_placement_uses_previous_values_without_dialogs(self):
        window = self.window
        window.barrier_kind_colors = {"gate": "#ff0000"}
        window.bridge_kind_colors = {"bridge": "#00ff00"}
        window.recent_barrier_kind = window.recent_pressure_plate_kind = "gate"
        window.recent_bridge_kind = window.recent_bridge_plate_kind = "bridge"
        window.recent_item_type = "health_potion"
        with (
            patch("map_editor.placement.KindDialog.prompt") as kind,
            patch("map_editor.items.ItemTypeDialog.prompt") as item,
        ):
            window.prompt_and_add_barrier_line((1, 1), (2, 1))
            window.prompt_and_add_pressure_plate(1, 1)
            window.prompt_and_add_light_bridge_rect((4, 4), (4, 4))
            window.prompt_and_add_bridge_plate(1, 1)
            window.prompt_and_add_item(1, 1)
            kind.assert_not_called()
            item.assert_not_called()
        self.assertEqual(window.map_data["items"][0]["type"], "health_potion")
        self.assertEqual(len(window.map_data["pressure_plates"]), 2)

    def test_nested_map_placement_reuses_configured_motion(self):
        window = self.window
        window.recent_nested_map = ("tile", 0, 3.0, 1.0, 0.0, (0, 0, 0), (0, 0, 0))
        with patch("map_editor.nested_maps.MotionDialog.prompt_nested") as prompt:
            window.add_nested_map((3, 3), (4, 3))
            window.add_nested_map((3, 5), (4, 5))
            prompt.assert_not_called()
        self.assertEqual([entry["travel_secs"] for entry in window.map_data["nested_maps"]], [3.0, 3.0])

    def test_actor_picker_rejects_unknown_kinds_and_toolbar_reuses_valid_choices(self):
        window = self.window
        kind = window.actor_kinds[0]
        dialog = ActorSpawnFieldsDialog(window, kind, 3)
        self.assertGreater(dialog._kind_edit.count(), 0)
        self.assertEqual(dialog.values(), (kind, 3))
        dialog.deleteLater()
        with (
            patch.object(ActorSpawnFieldsDialog, "exec", return_value=QDialog.DialogCode.Accepted),
            patch("map_editor.dialogs.catalogs.QMessageBox.warning") as warning,
        ):
            self.assertIsNone(ActorSpawnFieldsDialog.prompt(window, "not_a_kind", 3))
            warning.assert_called_once()
        window.mode_combo.setCurrentText(MODE_ACTOR_SPAWN_ZONE)
        window.recent_actor_spawn_kind = kind
        window.recent_actor_spawn_count = 7
        window.tool_settings.refresh()
        combos = window.tool_settings.findChildren(QComboBox)
        self.assertEqual(combos[0].currentText(), kind)
        with patch.object(ActorSpawnFieldsDialog, "prompt") as prompt:
            window.add_actor_spawn_zone_rect((2, 2), (3, 3))
            prompt.assert_not_called()
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], 7)

    def test_canvas_letter_shortcuts_do_not_steal_actor_search_text(self):
        window = self.window
        window.mode_combo.setCurrentText(MODE_ACTOR_SPAWN_ZONE)
        box = window.tool_settings.findChild(QComboBox)
        edit = box.lineEdit()
        edit.setFocus()
        edit.clear()
        window.canvas.viewport.fitted = False
        QTest.keyClicks(edit, "fml")
        self.assertEqual(edit.text(), "fml")
        self.assertFalse(window.canvas.viewport.fitted)
        self.assertFalse(window.show_material_overlay)
        self.assertFalse(window.show_adjacent_levels)
        window.canvas.setFocus()
        QTest.keyClick(window.canvas, Qt.Key.Key_F)
        self.assertTrue(window.canvas.viewport.fitted)
        self.assertEqual(window.adjacent_levels_action.shortcut().toString(), "L")
        QTest.keyClick(window.canvas, Qt.Key.Key_L)
        self.assertTrue(window.show_adjacent_levels)
        QTest.keyClick(window.canvas, Qt.Key.Key_L)
        self.assertFalse(window.show_adjacent_levels)
        QTest.keyClick(window.canvas, Qt.Key.Key_M, Qt.KeyboardModifier.ShiftModifier)
        self.assertFalse(window.show_adjacent_levels)

    def test_clicking_validation_issue_focuses_its_level_and_highlights_object(self):
        window = self.window
        data = insert_level_data(window.map_data, 1)
        data["levels"][1]["floors"] = [{"col": 6, "row": 5, "all": "unknown_alias"}]
        window.apply_change("Bad material", data)
        item = window.issues_panel.list.item(0)
        issue = item.data(Qt.ItemDataRole.UserRole)
        self.assertEqual((issue.level, issue.rect), (1, (6, 5, 7, 6)))
        window.issues_panel.list.itemClicked.emit(item)
        self.assertEqual(window.current_level, 1)
        self.assertEqual(window.canvas.issue_rects, [(6, 5, 7, 6)])
        center = window.canvas.viewport.from_grid(QPointF(6.5, 5.5))
        self.assertAlmostEqual(center.x(), window.canvas.width() / 2)

    def test_ramp_insertion_cancel_leaves_document_and_history_untouched(self):
        window = self.window
        data = insert_level_data(window.map_data, 1)
        data["ramps"] = [{"lower_level": 0, "low": [3, 3], "high": [6, 4], "all": DEFAULT_ALIAS}]
        window.apply_change("Ramp", data)
        before = copy.deepcopy(window.map_data)
        count = window.undo_stack.count()
        with patch("map_editor.structure.QMessageBox.question", return_value=QMessageBox.StandardButton.Cancel):
            window.add_level()
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), count)
        self.assertEqual(window.canvas.issue_rects, [])

    def test_file_notifications_invalidate_cached_nested_shapes(self):
        window = self.window
        window.nested_map_shapes = {"missing": None}
        window.dependencies.changed.emit()
        self.assertNotIn("missing", window.nested_map_shapes)
        watcher = MapDependencies(window)
        maps = Path(self.temp.name)
        nested = maps / "nested.json"
        with patch("map_editor.dependencies.MAPS_DIR", maps):
            watcher.watch(["nested"])
            changed = QSignalSpy(watcher.changed)
            self.assertIn(str(maps.resolve()), watcher.watcher.directories())
            QTimer.singleShot(100, lambda: write_map(nested, empty_map()))
            self.assertTrue(changed.wait(3000))
            watcher.watch(["nested"])
            self.assertIn(str(nested.resolve()), watcher.watcher.files())

    def test_large_map_fits_and_paints_with_invalid_nested_nudges(self):
        window = self.window
        data = paint_floors(empty_map(256, 256), 0, (250, 250, 256, 256), DEFAULT_ALIAS)
        data["nested_maps"] = [
            {"map": "missing", "level": 0, "from": [1, 1], "to": [2, 2], "from_nudge": [0], "to_nudge": [0]}
        ]
        window.doc.replace_with_new(data)
        window.canvas.fit_map()
        corner = window.canvas.viewport.from_grid(QPointF(256, 256))
        self.assertLessEqual(corner.x(), window.canvas.width())
        self.assertLessEqual(corner.y(), window.canvas.height())
        self.assertFalse(window.canvas.grab().isNull())

import copy
from unittest.mock import patch

from PySide6.QtCore import QPoint, QPointF, Qt
from PySide6.QtGui import QContextMenuEvent, QKeySequence, QWheelEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QComboBox, QMenu, QStatusBar

from editor_fixtures import WindowTestCase
from map_editor.constants import MODE_ACTOR_SPAWN_ZONE, MODE_ERASE
from map_editor.types import ZoneRef


class EditorNavigationTests(WindowTestCase):
    def zoom_for_panning(self):
        self.window.canvas.zoom_by(3)
        self.app.processEvents()
        area = self.window.canvas_scroll
        for bar in (area.horizontalScrollBar(), area.verticalScrollBar()):
            bar.setValue(bar.maximum() // 2)

    def scroll(self, *, pixels=QPoint(), angle=QPoint(), modifiers=Qt.KeyboardModifier.NoModifier, inverted=False):
        canvas = self.window.canvas
        position = QPointF(100, 100)
        self.app.sendEvent(canvas, QWheelEvent(
            position, QPointF(canvas.mapToGlobal(position.toPoint())), pixels, angle,
            Qt.MouseButton.NoButton, modifiers, Qt.ScrollPhase.ScrollUpdate, inverted,
        ))

    def test_pixel_gestures_pan_both_axes_without_zooming_or_editing(self):
        self.window.mode_combo.setCurrentText(MODE_ERASE)
        canvas = self.window.canvas
        self.zoom_for_panning()
        before = copy.deepcopy(self.window.map_data)
        cell = canvas.cell_size()
        origin = QPointF(canvas.viewport.offset)
        self.scroll(pixels=QPoint(-47, 32), angle=QPoint(0, 120), inverted=True)
        self.assertEqual(canvas.viewport.offset, origin + QPointF(-47, 32))
        self.assertEqual(canvas.cell_size(), cell)
        self.assertEqual(self.window.map_data, before)
        self.assertFalse(self.window.dirty)

    def test_mouse_wheel_pans_and_shift_wheel_pans_horizontally(self):
        canvas = self.window.canvas
        self.zoom_for_panning()
        origin = QPointF(canvas.viewport.offset)
        cell = canvas.cell_size()
        self.scroll(angle=QPoint(0, -120))
        self.assertEqual(canvas.viewport.offset, origin + QPointF(0, -40))
        self.scroll(angle=QPoint(0, -120), modifiers=Qt.KeyboardModifier.ShiftModifier)
        self.assertEqual(canvas.viewport.offset, origin + QPointF(-40, -40))
        self.scroll(angle=QPoint(120, 0))
        self.assertEqual(canvas.viewport.offset, origin + QPointF(0, -40))
        self.assertEqual(canvas.cell_size(), cell)

    def test_zoom_shortcuts_scroll_bars_and_fit_share_the_canvas_transform(self):
        canvas = self.window.canvas
        area = self.window.canvas_scroll
        canvas.setFocus()
        start_cell = canvas.cell_size()
        QTest.keySequence(canvas, QKeySequence(QKeySequence.StandardKey.ZoomIn))
        self.app.processEvents()
        self.assertGreater(canvas.cell_size(), start_cell)
        QTest.keySequence(canvas, QKeySequence(QKeySequence.StandardKey.ZoomOut))
        self.assertAlmostEqual(canvas.cell_size(), start_cell)

        canvas.zoom_by(3)
        self.app.processEvents()
        self.assertTrue(area.horizontalScrollBar().isVisible())
        self.assertTrue(area.verticalScrollBar().isVisible())
        area.horizontalScrollBar().setValue(area.horizontalScrollBar().maximum())
        area.verticalScrollBar().setValue(area.verticalScrollBar().maximum())
        self.assertAlmostEqual(canvas.viewport.offset.x(), -area.horizontalScrollBar().value(), delta=0.5)
        self.assertAlmostEqual(canvas.viewport.offset.y(), -area.verticalScrollBar().value(), delta=0.5)
        position = canvas.viewport.from_grid(QPointF(7.5, 7.5)).toPoint()
        self.assertTrue(canvas.rect().contains(position))
        QTest.mouseClick(canvas, Qt.MouseButton.LeftButton, pos=position)
        self.assertEqual(self.window.tile_selection, (7, 7, 8, 8))
        QTest.keyClick(canvas, Qt.Key.Key_F)
        self.app.processEvents()
        self.assertTrue(canvas.viewport.fitted)
        self.assertFalse(area.horizontalScrollBar().isVisible())
        self.assertFalse(area.verticalScrollBar().isVisible())
        corner = canvas.viewport.from_grid(QPointF(8, 8))
        self.assertLessEqual(corner.x(), canvas.width())
        self.assertLessEqual(corner.y(), canvas.height())

    def test_scroll_bars_follow_panning_and_window_resize(self):
        canvas = self.window.canvas
        area = self.window.canvas_scroll
        canvas.zoom_by(3)
        self.app.processEvents()
        self.scroll(pixels=QPoint(-31, -53))
        self.assertAlmostEqual(area.horizontalScrollBar().value(), -canvas.viewport.offset.x(), delta=0.5)
        self.assertAlmostEqual(area.verticalScrollBar().value(), -canvas.viewport.offset.y(), delta=0.5)
        self.window.resize(700, 600)
        self.app.processEvents()
        self.assertEqual(area.horizontalScrollBar().pageStep(), canvas.width())
        self.assertEqual(area.verticalScrollBar().pageStep(), canvas.height())

    def test_texture_host_controls_do_not_occupy_the_window(self):
        self.assertIsNone(self.window.findChild(QStatusBar))

    def test_panning_stops_at_map_edges_and_fit_recovers_keyboard_focus(self):
        canvas = self.window.canvas
        canvas.zoom_by(3)
        self.app.processEvents()
        self.scroll(pixels=QPoint(-10000, -10000))
        self.assertEqual(canvas.viewport.offset, QPointF(canvas.width() - 8 * canvas.cell_size(), canvas.height() - 8 * canvas.cell_size()))
        self.scroll(pixels=QPoint(10000, 10000))
        self.assertEqual(canvas.viewport.offset, QPointF())

        self.window.mode_combo.setCurrentText(MODE_ACTOR_SPAWN_ZONE)
        edit = self.window.tool_settings.findChild(QComboBox).lineEdit()
        edit.setFocus()
        self.scroll(pixels=QPoint(-10000, -10000))
        self.assertIs(QApplication.focusWidget(), canvas)
        QTest.keyClick(canvas, Qt.Key.Key_F)
        self.app.processEvents()
        self.assertTrue(canvas.viewport.fitted)
        self.assertEqual(canvas.viewport.offset, QPointF())
        self.scroll(pixels=QPoint(-10000, 10000))
        self.assertEqual(canvas.viewport.offset, QPointF())
        self.assertTrue(canvas.viewport.fitted)


class SpawnZoneHandleTests(WindowTestCase):
    def test_right_click_selection_can_resize_without_option_and_undo(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["actor_spawn_zones"] = [{"level": 0, "cols": [2, 4], "rows": [2, 4], "kind": "zapper", "count": 3}]
        window.apply_change("Spawn zone", data)
        canvas = window.canvas
        canvas.zoom_by(1.2)
        canvas.pan_by(QPointF(30, 20))
        point = canvas.viewport.from_grid(QPointF(3, 3)).toPoint()
        menu = QMenu(canvas)
        with patch("map_editor.canvas.QMenu", return_value=menu), patch.object(menu, "exec"):
            canvas.contextMenuEvent(QContextMenuEvent(QContextMenuEvent.Reason.Mouse, point, canvas.mapToGlobal(point)))
        self.assertEqual(window.selected_spawn_zone_ref, ZoneRef("actor_spawn_zones", 0))
        start = canvas.viewport.from_grid(QPointF(4, 4)).toPoint()
        end = canvas.viewport.from_grid(QPointF(5, 6)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
        self.assertEqual(window.spawn_zone_drag.handle, "se")
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
        zone = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["cols"], zone["rows"]), ([2, 5], [2, 6]))
        self.assertEqual((zone["kind"], zone["count"]), ("zapper", 3))
        window.undo_stack.undo()
        self.assertEqual(window.map_data["actor_spawn_zones"], data["actor_spawn_zones"])
        window.undo_stack.redo()
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["cols"], [2, 5])

    def test_player_zone_side_handle_and_normal_tile_selection(self):
        window = self.window
        window.add_player_spawn_zone_rect((2, 2), (3, 3))
        canvas = window.canvas
        start = canvas.viewport.from_grid(QPointF(4, 3)).toPoint()
        end = canvas.viewport.from_grid(QPointF(5, 3)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
        self.assertEqual(window.map_data["player_spawn_zones"][0]["cols"], [2, 5])
        QTest.mouseClick(canvas, Qt.MouseButton.LeftButton, pos=canvas.viewport.from_grid(QPointF(6.5, 6.5)).toPoint())
        self.assertIsNone(window.selected_spawn_zone_ref)
        self.assertEqual(window.tile_selection, (6, 6, 7, 7))

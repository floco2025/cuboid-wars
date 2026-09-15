"""Tool discovery and operation changes through the actual editor widgets."""

from PySide6.QtCore import QPoint, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QComboBox

from editor_fixtures import WindowTestCase, furnished_map
from map_editor import constants as c
from map_editor.window import EditorWindow


class ToolPaletteTests(WindowTestCase):
    def button(self, mode):
        return self.window.tool_palette.buttons[mode]

    def choose(self, mode):
        QTest.mouseClick(self.button(mode), Qt.MouseButton.LeftButton)

    def open_search(self):
        self.window.canvas.setFocus()
        QTest.keyClick(self.window.canvas, Qt.Key.Key_K, Qt.KeyboardModifier.ControlModifier)
        self.app.processEvents()
        self.assertTrue(self.window.tool_search.isVisible())
        return self.window.tool_search

    def test_palette_click_selects_tool_and_returns_keyboard_to_canvas(self):
        self.choose(c.MODE_LIGHT_BRIDGE)
        palette = self.window.tool_palette
        self.assertEqual(self.window.mode, c.MODE_LIGHT_BRIDGE)
        self.assertTrue(self.button(c.MODE_LIGHT_BRIDGE).isChecked())
        self.assertTrue(palette.place_button.isChecked())
        self.assertEqual(palette.current_button.text(), c.MODE_LIGHT_BRIDGE)
        self.assertTrue(self.window.canvas.hasFocus())
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Right)
        self.assertEqual(self.window.mode, c.MODE_PRESSURE_PLATE)
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Left)
        self.assertEqual(self.window.mode, c.MODE_LIGHT_BRIDGE)

    def test_wall_erase_keeps_floor_and_items_and_undo_restores_support_light(self):
        window = self.window
        window.doc.replace_with_new(furnished_map())
        self.choose(c.MODE_WALL)
        QTest.keyClick(window.canvas, Qt.Key.Key_E)
        self.assertEqual(window.mode, c.MODE_ERASE_WALLS)
        self.assertTrue(window.tool_palette.erase_button.isChecked())
        self.assertTrue(self.button(c.MODE_WALL).isChecked())
        self.assertEqual(window.canvas.cursor().shape(), Qt.CursorShape.ForbiddenCursor)
        self.click(1, 1)
        self.assertEqual(window.map_data["levels"][0]["walls"], [])
        self.assertEqual(window.map_data["levels"][0]["lights"], [])
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 2)
        self.assertEqual(len(window.map_data["items"]), 1)
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["levels"][0]["walls"]), 1)
        self.assertEqual(len(window.map_data["levels"][0]["lights"]), 1)
        self.choose(c.MODE_FLOOR)
        self.assertEqual(window.mode, c.MODE_FLOOR)
        self.assertTrue(window.tool_palette.place_button.isChecked())

    def test_shared_erase_operations_return_to_the_selected_variant(self):
        palette = self.window.tool_palette
        for mode, erase in (
            (c.MODE_INACCESSIBLE_FLOOR, c.MODE_ERASE_FLOORS),
            (c.MODE_RAMP_DOWN, c.MODE_ERASE_RAMPS),
            (c.MODE_PLAYER_SPAWN_ZONE, c.MODE_ERASE_SPAWN_ZONES),
        ):
            with self.subTest(mode=mode):
                self.choose(mode)
                QTest.mouseClick(palette.erase_button, Qt.MouseButton.LeftButton)
                self.assertEqual(self.window.mode, erase)
                self.assertTrue(self.button(mode).isChecked())
                QTest.mouseClick(palette.place_button, Qt.MouseButton.LeftButton)
                self.assertEqual(self.window.mode, mode)
        self.choose(c.MODE_SELECT)
        self.assertFalse(palette.erase_button.isEnabled())
        self.choose(c.MODE_FLOOR_MATERIAL)
        self.assertFalse(palette.erase_button.isEnabled())

    def test_keep_floors_is_available_on_general_erase_and_preserves_supported_items(self):
        window = self.window
        window.doc.replace_with_new(furnished_map())
        self.choose(c.MODE_ERASE)
        keep = window.tool_palette.keep_floors
        self.assertTrue(keep.isVisible())
        QTest.mouseClick(keep, Qt.MouseButton.LeftButton, pos=QPoint(8, keep.height() // 2))
        self.assertEqual(window.mode, c.MODE_ERASE_KEEP_FLOORS)
        canvas = window.canvas
        cell = canvas.cell_size()
        start = QPoint(round(1.5 * cell), round(1.5 * cell))
        end = QPoint(round(2.5 * cell), round(2.5 * cell))
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
        QTest.mouseMove(canvas, end)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
        self.assertEqual(window.map_data["levels"][0]["walls"], [])
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 2)
        self.assertEqual(len(window.map_data["items"]), 1)
        self.assertEqual(len(window.map_data["pressure_plates"]), 1)
        QTest.mouseClick(keep, Qt.MouseButton.LeftButton, pos=QPoint(8, keep.height() // 2))
        self.assertEqual(window.mode, c.MODE_ERASE)

    def test_search_reaches_every_existing_canvas_mode(self):
        search = self.open_search()
        expected = {value for name, value in vars(c).items() if name.startswith("MODE_")}
        modes = [search.results.item(row).data(Qt.ItemDataRole.UserRole) for row in range(search.results.count())]
        self.assertEqual(set(modes), expected)
        self.assertEqual(len(modes), len(expected))
        for mode in modes:
            with self.subTest(mode=mode):
                search.show_for(mode)
                QTest.keyClick(search.query, Qt.Key.Key_Return)
                self.app.processEvents()
                self.assertFalse(search.isVisible())
                self.assertEqual(self.window.mode, mode)
                self.assertTrue(self.window.canvas.hasFocus())
                self.assertTrue(self.window.tool_palette.current_button.icon().availableSizes())
        self.assertFalse(self.window.dirty)

    def test_search_filters_words_in_any_order_and_enter_selects_erase_directly(self):
        self.choose(c.MODE_RAMP_DOWN)
        search = self.open_search()
        QTest.keyClicks(search.query, "ramps erase")
        self.assertEqual(search.results.count(), 1)
        QTest.keyClick(search.query, Qt.Key.Key_Return)
        self.app.processEvents()
        self.assertEqual(self.window.mode, c.MODE_ERASE_RAMPS)
        QTest.keyClick(self.window.canvas, Qt.Key.Key_E)
        self.assertEqual(self.window.mode, c.MODE_RAMP_DOWN)

    def test_search_arrows_choose_a_match_and_escape_or_empty_results_do_not_change_tool(self):
        search = self.open_search()
        QTest.keyClicks(search.query, "bridge")
        self.assertEqual(search.results.count(), 2)
        QTest.keyClick(search.query, Qt.Key.Key_Down)
        self.assertEqual(search.query.text(), "bridge")
        self.assertEqual(search.results.currentItem().data(Qt.ItemDataRole.UserRole), c.MODE_ERASE_LIGHT_BRIDGES)
        QTest.keyClick(search.query, Qt.Key.Key_Up)
        QTest.keyClick(search.query, Qt.Key.Key_Return)
        self.app.processEvents()
        self.assertEqual(self.window.mode, c.MODE_LIGHT_BRIDGE)
        search = self.open_search()
        QTest.keyClicks(search.query, "not-a-tool")
        self.assertTrue(search.empty.isVisible())
        QTest.keyClick(search.query, Qt.Key.Key_Return)
        self.app.processEvents()
        self.assertTrue(search.isVisible())
        self.assertEqual(self.window.mode, c.MODE_LIGHT_BRIDGE)
        QTest.keyClick(search.query, Qt.Key.Key_Escape)
        self.assertFalse(search.isVisible())
        self.assertEqual(self.window.mode, c.MODE_LIGHT_BRIDGE)

    def test_search_typing_and_property_typing_do_not_trigger_canvas_shortcuts(self):
        self.choose(c.MODE_ACTOR_SPAWN_ZONE)
        edit = self.window.tool_settings.findChild(QComboBox).lineEdit()
        edit.setFocus()
        edit.clear()
        QTest.keyClicks(edit, "erase")
        self.assertEqual(edit.text(), "erase")
        self.assertEqual(self.window.mode, c.MODE_ACTOR_SPAWN_ZONE)
        search = self.open_search()
        QTest.keyClicks(search.query, "fmlre")
        self.assertEqual(search.query.text(), "fmlre")
        self.assertEqual(self.window.mode, c.MODE_ACTOR_SPAWN_ZONE)
        self.assertFalse(self.window.show_material_overlay)
        self.assertFalse(self.window.show_adjacent_levels)
        self.assertFalse(self.window.show_roam_extensions)
        search.reject()

    def test_collapsed_palette_still_has_search_and_remembers_visibility(self):
        window = self.window
        window.tool_palette.toggleViewAction().trigger()
        self.assertTrue(window.tool_palette.isHidden())
        QTest.mouseClick(window.tool_palette.current_button, Qt.MouseButton.LeftButton)
        self.assertTrue(window.tool_search.isVisible())
        window.tool_search.reject()
        window.close()
        self.window = EditorWindow(self.path, preferences=window.preferences)
        window.deleteLater()
        self.window._autosave_timer.stop()
        self.window.show()
        self.app.processEvents()
        self.assertTrue(self.window.tool_palette.isHidden())
        self.window.tool_palette.toggleViewAction().trigger()
        self.assertTrue(self.window.tool_palette.isVisible())

    def test_switching_tool_cancels_a_pending_canvas_drag(self):
        self.choose(c.MODE_FLOOR)
        canvas = self.window.canvas
        cell = canvas.cell_size()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=QPoint(round(3.5 * cell), round(3.5 * cell)))
        self.assertIsNotNone(canvas.drag_start_cell)
        self.choose(c.MODE_WALL)
        self.assertIsNone(canvas.drag_start_cell)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=QPoint(round(4.5 * cell), round(4.5 * cell)))
        self.assertFalse(self.window.dirty)

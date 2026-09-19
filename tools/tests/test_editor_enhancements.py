"""Object selection and the shared, compact editing panels."""

import copy
from unittest.mock import Mock, patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest

from editor_fixtures import WindowTestCase, floor, furnished_map, nested
from map_editor import constants as c
from map_editor.elements import ElementRef
from map_editor.normalization import empty_level, empty_map
from map_editor.window import EditorWindow


class EditorEnhancementTests(WindowTestCase):
    def drag(self, start, end):
        canvas = self.window.canvas
        a = canvas.viewport.from_grid(QPointF(*start)).toPoint()
        b = canvas.viewport.from_grid(QPointF(*end)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=a)
        QTest.mouseMove(canvas, b)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=b)
        self.app.processEvents()

    def item_map(self):
        data = empty_map(8, 8)
        data["levels"][0]["floors"] = [floor(i, i) for i in (1, 3, 5)]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "gold"}]
        self.window.doc.replace_with_new(data)
        return copy.deepcopy(self.window.map_data)

    def test_click_selects_one_object_and_delete_preserves_its_floor(self):
        window = self.window
        before = self.item_map()
        for key in (Qt.Key.Key_Delete, Qt.Key.Key_Backspace):
            with self.subTest(key=key):
                self.drag((1.5, 1.5), (1.5, 1.5))
                self.assertEqual(window.selection_refs(), [ElementRef("items", 0)])
                self.assertTrue(window.properties_panel.isVisible())
                window.canvas.setFocus()
                QTest.keyClick(window.canvas, key)
                self.assertEqual(window.map_data["items"], [])
                self.assertEqual(window.map_data["levels"], before["levels"])
                window.undo_stack.undo()
                self.assertEqual(window.map_data, before)
                self.assertEqual(window.selection_refs(), [])

    def test_clicking_an_object_keeps_neighboring_floor_tiles_visible(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(col, row) for col in range(8) for row in range(8)]
        data["items"] = [{"level": 0, "col": 3, "row": 3, "type": "gold"}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        canvas = window.canvas
        canvas.viewport.fitted = False
        canvas.viewport.cell = 24
        canvas.viewport.offset = QPointF(30, 30)
        self.app.processEvents()
        point = canvas.viewport.from_grid(QPointF(3.5, 3.5)).toPoint()
        neighbors = [canvas.viewport.from_grid(QPointF(col + 0.5, 4.5)).toPoint() for col in range(3, 7)]
        initial = canvas.grab().toImage()
        colors = [initial.pixelColor(point) for point in neighbors]
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=point)
        pressed = canvas.grab().toImage()
        self.assertEqual([pressed.pixelColor(point) for point in neighbors], colors)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=point)
        selected = canvas.grab().toImage()
        self.assertEqual([selected.pixelColor(point) for point in neighbors], colors)
        self.assertEqual(window.map_data, before)

    def test_property_undo_does_not_expand_an_item_selection_to_its_floor(self):
        window = self.window
        before = self.item_map()
        window.inspect_hit((c.HIT_ITEM, (1, 1)), show=True)
        self.assertTrue(window.properties_panel.isVisible())
        self.assertTrue(window.delete_action.isEnabled())
        self.set_property("type", "health_potion")
        self.assertEqual(window.selection_refs(), [ElementRef("items", 0)])
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
        window.delete_selection()
        self.assertEqual(window.map_data, before)

    def test_drag_selects_whole_intersected_zones_across_their_level_span(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] += [empty_level(1), empty_level(2)]
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "levels": 3,
                "cols": [1, 4],
                "rows": [1, 4],
                "kind": "scuttler",
                "count": [1],
                "respawn_secs": None,
            }
        ]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        self.drag((0.5, 0.5), (2.5, 2.5))
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])
        window.copy_selection()
        self.assertEqual(len(window.tile_clipboard["levels"]), 3)
        self.assertEqual(window.tile_clipboard["actor_spawn_zones"][0]["cols"], [0, 3])
        window.delete_selection()
        self.assertEqual(window.map_data["actor_spawn_zones"], [])
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_object_copy_move_and_duplicate_preserve_background_and_refuse_occupied_destinations(self):
        window = self.window
        before = self.item_map()
        window.inspect_hit((c.HIT_ITEM, (1, 1)))
        window.copy_selection()
        self.assertTrue(window.clipboard_objects)
        self.assertEqual(window.tile_clipboard["levels"][0]["floors"], [])
        window.set_tile_selection((3, 3, 4, 4), objects=True)
        window.paste_selection()
        self.assertEqual([(item["col"], item["row"]) for item in window.map_data["items"]], [(1, 1), (3, 3)])
        self.assertEqual(window.map_data["levels"], before["levels"])
        self.assertEqual(window.selection_refs(), [ElementRef("items", 1)])
        window.duplicate_selection()
        window.move_pending_block(QPointF(5.5, 5.5))
        window.commit_pending_block()
        self.assertEqual(len(window.map_data["items"]), 3)
        self.assertEqual(window.map_data["levels"], before["levels"])
        window.undo_stack.undo()
        window.inspect_hit((c.HIT_ITEM, (3, 3)))
        self.drag((3.5, 3.5), (5.5, 5.5))
        self.assertEqual([(item["col"], item["row"]) for item in window.map_data["items"]], [(1, 1), (5, 5)])
        self.assertEqual(window.map_data["levels"], before["levels"])
        occupied = copy.deepcopy(window.map_data)
        window.duplicate_selection()
        window.move_pending_block(QPointF(1.5, 1.5))
        with patch.object(window, "notify") as notify:
            window.commit_pending_block()
        self.assertIn("already contains", notify.call_args.args[0])
        self.assertEqual(window.map_data, occupied)
        window.clear_selection()

    def test_deleting_cutting_or_moving_a_support_floor_takes_what_stands_on_it(self):
        window = self.window
        before = self.item_map()
        data = copy.deepcopy(before)
        data["levels"][0]["floors"].append(floor(6, 5))
        data["switches"] = [{"id": "gate", "activation": "toggle", "reset_on_player_death": "never"}]
        data["pressure_plates"] = [
            {"level": 0, "col": 3, "row": 3, "switch": "gate"},
            {"level": 0, "col": 5, "row": 5, "switch": "gate"},
        ]
        window.doc.replace_with_new(data)
        window.inspect_hit((c.HIT_FLOOR, (1, 1)))
        window.delete_selection()
        self.assertEqual(window.map_data["items"], [])
        self.assertNotIn((1, 1), [(f["col"], f["row"]) for f in window.map_data["levels"][0]["floors"]])
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["items"]), 1)
        window.inspect_hit((c.HIT_FLOOR, (3, 3)))
        window.cut_selection()
        self.assertEqual([(p["col"], p["row"]) for p in window.map_data["pressure_plates"]], [(5, 5)])
        self.assertEqual(len(window.tile_clipboard["pressure_plates"]), 1)
        # A press on the plate's cell picks the plate, so the move starts on the bare floor.
        floors = window.map_data["levels"][0]["floors"]
        window.inspect_refs(
            [ElementRef("floors", i, 0) for i, f in enumerate(floors) if (f["col"], f["row"]) in ((5, 5), (6, 5))]
        )
        self.drag((6.5, 5.5), (6.5, 2.5))
        self.assertEqual([(p["col"], p["row"]) for p in window.map_data["pressure_plates"]], [(5, 2)])
        self.assertIn((5, 2), [(f["col"], f["row"]) for f in window.map_data["levels"][0]["floors"]])

    def test_actor_properties_edit_first_level_counts_respawn_roam_and_controls_in_one_undo(self):
        window = self.window
        data = furnished_map()
        data["switches"] = [{"id": "barrier_1", "activation": "toggle", "reset_on_player_death": "never"}]
        window.switch_ids = ["barrier_1"]
        data["checkpoints"] = []
        data["levels"] += [empty_level(1), empty_level(2)]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [3, 4], "rows": [3, 4], "kind": "scuttler", "count": [1], "respawn_secs": None}
        ]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)), show=True)
        for key, value in {
            "level": 1,
            "levels": 2,
            "kind": "zapper",
            "count": "0, 2, 4",
            "respawn_secs": 12.5,
            "beam_in_secs": 2.5,
            "roam_distance": 3.5,
            "switch": "barrier_1",
            "switch_inverted": True,
        }.items():
            self.set_property(key, value)
        actor = window.map_data["actor_spawn_zones"][0]
        self.assertEqual(
            (
                actor["level"],
                actor["levels"],
                actor["kind"],
                actor["count"],
                actor["respawn_secs"],
                actor["beam_in_secs"],
                actor["roam_distance"],
                actor["switch"],
                actor["switch_inverted"],
            ),
            (1, 2, "zapper", [0, 2, 4], 12.5, 2.5, 3.5, "barrier_1", True),
        )
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_nested_motion_fields_edit_together_and_validate_without_discarding_input(self):
        window = self.window
        data = furnished_map()
        data["switches"] = [{"id": "barrier_1", "activation": "toggle", "reset_on_player_death": "never"}]
        window.switch_ids = ["barrier_1"]
        data["levels"].append(empty_level(1))
        data["nested_geometry"] = {"room": empty_map(1, 1)}
        data["nested_maps"] = [nested("room", 0, [3, 3], [5, 5])]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.inspect_refs([ElementRef("nested_maps", 0)], show=True)
        self.assertTrue(window.properties_panel.widgets[("pause_secs",)].isEnabled())
        values = {
            "travel_secs": 0,
            "to_level": 1,
            "pause_secs": 2,
            "phase_secs": 1,
            "switch": "barrier_1",
            "switch_inverted": True,
            "motion": "follow_switch",
        }
        values.update({(end, axis): 0.25 * (axis + 1) for end in ("from_nudge", "to_nudge") for axis in range(3)})
        for key, value in values.items():
            self.set_property(key, value)
        panel = window.properties_panel
        self.assertEqual(window.map_data, before)
        self.assertTrue(panel.error.isVisible())
        self.set_property("travel_secs", 3)
        entry = window.map_data["nested_maps"][0]
        self.assertEqual(
            (entry["to_level"], entry["travel_secs"], entry["pause_secs"], entry["phase_secs"]), (1, 3, 2, 1)
        )
        self.assertEqual(entry["from_nudge"], [0.25, 0.5, 0.75])
        self.assertEqual(entry["to_nudge"], entry["from_nudge"])
        self.assertEqual((entry["switch"], entry["switch_inverted"]), ("barrier_1", True))
        self.assertEqual(entry["motion"], "follow_switch")
        self.assertFalse(panel.widgets[("pause_secs",)].isEnabled())
        self.assertFalse(panel.widgets[("phase_secs",)].isEnabled())
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
        window.undo_stack.redo()
        window.inspect_refs([ElementRef("nested_maps", 0)], show=True)
        self.set_property("switch", None)
        self.assertTrue(panel.error.isVisible())
        self.assertEqual(window.map_data["nested_maps"][0]["switch"], "barrier_1")
        self.set_property("motion", "cycle")
        self.assertTrue(panel.widgets[("pause_secs",)].isEnabled())
        self.assertTrue(panel.widgets[("phase_secs",)].isEnabled())
        entry = window.map_data["nested_maps"][0]
        self.assertNotIn("switch", entry)
        self.assertEqual((entry["pause_secs"], entry["phase_secs"]), (2, 1))

    def test_connections_are_opt_in_and_follow_the_current_selection(self):
        window = self.window
        data = furnished_map()
        data["switches"] = [{"id": "barrier_1", "activation": "toggle", "reset_on_player_death": "never"}]
        window.switch_ids = ["barrier_1"]
        data["levels"][0]["barriers"] = [
            {"c0": 3, "r0": 3, "c1": 4, "r1": 3, "kind": "barrier_1", "switch": "barrier_1"}
        ]
        data["fireworks"] = {"switch": "barrier_1", "cooldown_secs": 10}
        window.doc.replace_with_new(data)
        overlay = window.connection_overlay
        window.inspect_hit((c.HIT_PRESSURE_PLATE, (1, 1)))
        self.assertEqual(len(overlay.connections), 3)
        painter = Mock()
        overlay.paint(painter, 30)
        self.assertEqual(painter.mock_calls, [])
        window.connections_action.trigger()
        overlay.paint(painter, 30)
        painter.drawLine.assert_called_once()
        window.connections_action.trigger()
        painter.reset_mock()
        overlay.paint(painter, 30)
        self.assertEqual(painter.mock_calls, [])
        window.inspect_hit((c.HIT_FLOOR, (2, 2)))
        window.connections_action.trigger()
        overlay.paint(painter, 30)
        self.assertEqual(painter.mock_calls, [])
        window.inspect_hit((c.HIT_PRESSURE_PLATE, (1, 1)))
        overlay.paint(painter, 30)
        painter.drawLine.assert_called_once()

    def test_permanent_panel_widths_and_icon_mode_survive_reopening(self):
        window = self.window
        window.inspect_hit((c.HIT_FLOOR, (1, 1)), show=True)
        window.resizeDocks([window.tool_palette, window.properties_panel], [190, 245], Qt.Orientation.Horizontal)
        self.app.processEvents()
        widths = (window.tool_palette.width(), window.properties_panel.width())
        window.tool_icons_action.trigger()
        self.app.processEvents()
        self.assertLess(window.tool_palette.width(), widths[0])
        window.activate_tool(c.MODE_WALL)
        QTest.keyClick(window.canvas, Qt.Key.Key_E)
        self.assertEqual(window.mode, c.MODE_ERASE_WALLS)
        other = EditorWindow(self.path, preferences=window.preferences)
        other._autosave_timer.stop()
        try:
            other.show()
            self.app.processEvents()
            self.assertTrue(other.tool_palette.icon_only)
            self.assertTrue(other.tool_icons_action.isChecked())
            self.assertEqual(other.tool_palette.width(), window.tool_palette.width())
            self.assertEqual(other.properties_panel.width(), widths[1])
        finally:
            other.close()
            other.deleteLater()

    def test_rotating_objects_with_nested_geometry_preserves_unselected_dependents(self):
        window = self.window
        data = self.item_map()
        data["nested_geometry"] = {"room": empty_map(2, 1)}
        data["nested_maps"] = [nested("room", 0, [3, 1], [3, 1])]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.inspect_refs([ElementRef("floors", 0, 0), ElementRef("nested_maps", 0)])
        window.transform_selection("rotate")
        self.assertTrue(window.pending_block.additions)
        window.move_pending_block(QPointF(5.5, 3.5))
        with patch.object(window, "notify") as notify:
            window.commit_pending_block()
        self.assertTrue(notify.called)
        self.assertEqual(window.map_data, before)
        self.assertIsNotNone(window.pending_block)

    def test_object_paste_lands_relative_to_the_viewed_level(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"].append(empty_level(1))
        data["levels"][0]["floors"] = [floor(1, 1), floor(6, 6)]
        data["ladders"] = [{"col": 1, "row": 1, "side": "S", "lower_level": 0, "levels": 1}]
        window.doc.replace_with_new(data)
        window.select_level(1)
        window.inspect_refs([ElementRef("ladders", 0)])
        window.copy_selection()
        window.set_tile_selection((6, 6, 7, 7), objects=True)
        window.paste_selection()
        ladders = window.map_data["ladders"]
        self.assertEqual([(l["col"], l["row"], l["lower_level"]) for l in ladders], [(1, 1, 0), (6, 6, 0)])
        self.assertEqual(len(window.map_data["levels"]), 2)
        window.select_level(0)
        window.set_tile_selection((4, 4, 5, 5), objects=True)
        with patch.object(window, "notify") as notify:
            window.paste_selection()
        self.assertIn("outside the map", notify.call_args.args[0])
        self.assertEqual(len(window.map_data["ladders"]), 2)

    def test_clearing_a_switch_in_properties_drops_its_response_too(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["switches"] = [{"id": "barrier_1", "activation": "toggle", "reset_on_player_death": "never"}]
        window.switch_ids = ["barrier_1"]
        data["nested_geometry"] = {"room": empty_map(1, 1)}
        data["nested_maps"] = [{**nested("room", 0, [3, 3], [5, 5]), "switch": "barrier_1", "switch_inverted": True}]
        window.doc.replace_with_new(data)
        window.inspect_refs([ElementRef("nested_maps", 0)], show=True)
        self.set_property("switch", None)
        entry = window.map_data["nested_maps"][0]
        self.assertNotIn("switch", entry)
        self.assertNotIn("switch_inverted", entry)

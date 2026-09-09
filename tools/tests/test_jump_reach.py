import copy
from dataclasses import replace
from math import sqrt
import unittest
from unittest.mock import patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest

from editor_fixtures import WindowTestCase
from map_editor.catalogs import load_map_settings
from map_editor.constants import MODE_FLOOR, MODE_FLOOR_MATERIAL, MODE_JUMP_REACH, MODE_SELECT
from map_editor.jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED, JumpSettings, calculate_reach, landing_time
from map_editor.normalization import empty_level, empty_map
from map_editor.transforms import insert_level_data, resize_map_data


class JumpReachTests(unittest.TestCase):
    settings = JumpSettings(1, 1, 2, 0.5, 1, 2, 2, 1, 0)

    def reach(self, *, margin=0, running=True, settings=None, origin=(1, 5, 5)):
        data = empty_map(20, 20)
        data["levels"] = [empty_level(i) for i in range(5)]
        return calculate_reach(settings or self.settings, origin, data, running=running, margin=margin)

    def test_four_combinations_have_independent_ranges(self):
        reach = self.reach()
        self.assertEqual(reach[1, 8, 5], NORMAL | SPEED | ANTI_GRAVITY | BOTH)
        self.assertEqual(reach[1, 9, 5], SPEED | ANTI_GRAVITY | BOTH)
        self.assertEqual(reach[1, 11, 5], BOTH)
        self.assertNotIn((1, 15, 5), reach)

    def test_shortest_square_gap_includes_diagonals_and_touching_corners(self):
        reach = self.reach()
        for col, row in ((5, 6), (6, 6), (7, 7), (8, 5)):
            self.assertTrue(reach[1, col, row] & NORMAL)
        self.assertFalse(reach[1, 8, 7] & NORMAL)

    def test_descending_landing_above_below_and_at_apex(self):
        self.assertEqual(landing_time(2, 2, 0), 2)
        self.assertEqual(landing_time(2, 2, 1), 1)
        self.assertAlmostEqual(landing_time(2, 2, -2), 1 + sqrt(3))
        self.assertIsNone(landing_time(2, 2, 1.001))
        reach = self.reach()
        self.assertTrue(reach[2, 7, 5] & NORMAL)
        self.assertFalse(reach[3, 7, 5] & (NORMAL | SPEED))
        self.assertEqual(reach[3, 7, 5], ANTI_GRAVITY | BOTH)
        self.assertNotIn((4, 5, 5), reach)

    def test_margin_boundaries_and_smaller_walk_range(self):
        self.assertTrue(self.reach(margin=0.999)[1, 7, 5] & NORMAL)
        self.assertTrue(self.reach(margin=1)[1, 7, 5] & NORMAL)
        self.assertFalse(self.reach(margin=1.001)[1, 7, 5] & NORMAL)
        self.assertFalse(self.reach(running=False)[1, 8, 5] & NORMAL)
        self.assertTrue(self.reach()[1, 8, 5] & NORMAL)
        self.assertEqual(self.reach(margin=100), {})

    def test_margin_can_exceed_one_tiles_width(self):
        settings = replace(self.settings, run_speed=4)
        reach = self.reach(settings=settings, margin=0.5)
        self.assertTrue(reach[1, 12, 5] & NORMAL)
        self.assertFalse(reach[1, 13, 5] & NORMAL)

    def test_zero_gravity_does_not_land(self):
        self.assertIsNone(landing_time(2, 0, -2))
        reach = self.reach(settings=replace(self.settings, low_gravity=0))
        self.assertTrue(reach)
        self.assertTrue(all(mask & (ANTI_GRAVITY | BOTH) == 0 for mask in reach.values()))

    def test_results_stay_in_bounds_and_origin_has_no_markers(self):
        reach = self.reach(origin=(0, 0, 0))
        self.assertNotIn((0, 0, 0), reach)
        self.assertTrue(all(0 <= level < 5 and 0 <= col < 20 and 0 <= row < 20 for level, col, row in reach))

    def test_invalid_fields_name_the_source_and_field(self):
        settings = {
            "geometry": {"grid_cell_size": 1, "level_height": 1, "wall_thickness": 0.2},
            "movement": {"gravity": 2, "low_gravity": 1, "player": {
                "jump_speed": 2, "walk_speed": 0.5, "run_speed": 1, "speed_power_up": 2,
            }},
        }
        self.assertEqual(JumpSettings.from_settings(settings, "settings.json"), replace(self.settings, wall_thickness=0.2))
        for value in (None, True, "1", 0, -1, float("nan"), float("inf")):
            with self.subTest(value=value):
                invalid = copy.deepcopy(settings)
                invalid["movement"]["player"]["run_speed"] = value
                with self.assertRaisesRegex(ValueError, r"settings.json: movement.player.run_speed"):
                    JumpSettings.from_settings(invalid, "settings.json")
        settings["movement"]["low_gravity"] = 0
        self.assertEqual(JumpSettings.from_settings(settings, "settings.json").low_gravity, 0)
        settings["movement"]["player"] = None
        with self.assertRaisesRegex(ValueError, "movement.player.jump_speed"):
            JumpSettings.from_settings(settings, "settings.json")

    def test_invalid_margin_is_rejected(self):
        for margin in (-0.1, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                self.reach(margin=margin)


class JumpReachWindowTests(WindowTestCase):
    def select_origin(self, col=2, row=2):
        self.window.mode_combo.setCurrentText(MODE_JUMP_REACH)
        self.app.processEvents()
        self.click(col, row)

    def test_click_empty_cell_and_replace_origin_without_editing_document(self):
        before = copy.deepcopy(self.window.doc.root_data)
        self.select_origin()
        overlay = self.window.jump_reach
        self.assertEqual(overlay.origin, (0, 2, 2))
        self.assertTrue(overlay.results)
        self.click(4, 3)
        self.assertEqual(overlay.origin, (0, 4, 3))
        self.assertEqual(self.window.doc.root_data, before)
        self.assertFalse(self.window.dirty)
        self.assertEqual(self.window.undo_stack.count(), 0)

    def test_overlay_survives_editing_navigation_undo_and_escape(self):
        data = insert_level_data(self.window.map_data, 1)
        self.window.doc.replace_with_new(data)
        self.select_origin()
        overlay = self.window.jump_reach
        results = overlay.results
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.app.processEvents()
        self.click(4, 4)
        self.window.undo_stack.undo()
        self.window.undo_stack.redo()
        self.assertIsNot(overlay.results, results)
        results = overlay.results
        self.window.select_level(1)
        self.window.canvas.zoom_by(1.25)
        self.window.canvas.pan_by(QPointF(-10, -10))
        self.window.canvas.setFocus()
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Escape)
        self.assertEqual(overlay.origin, (0, 2, 2))
        self.assertIs(overlay.results, results)
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertTrue(overlay.clear_button.isVisible())
        self.assertTrue(any(level == 1 for level, _, _ in results))
        overlay.clear_action.trigger()
        self.assertIsNone(overlay.origin)
        self.assertEqual(overlay.results, {})
        self.assertFalse(overlay.toolbar.isVisible())

    def test_margin_precision_distance_display_and_movement_recompute(self):
        self.select_origin()
        overlay = self.window.jump_reach
        results = overlay.results
        overlay.margin.setValue(0.123)
        self.assertAlmostEqual(overlay.margin.value(), 0.123)
        overlay.margin.stepUp()
        self.assertAlmostEqual(overlay.margin.value(), 0.133)
        self.assertIsNot(overlay.results, results)
        overlay.movement.setCurrentText("Walk")
        self.assertEqual(overlay.distance.text(), "0.80 m normal / 1.20 m speed")
        self.window.mode_combo.setCurrentText(MODE_SELECT)
        self.window.mode_combo.setCurrentText(MODE_JUMP_REACH)
        self.assertAlmostEqual(overlay.margin.value(), 0.133)
        self.assertEqual(overlay.movement.currentText(), "Walk")

    def test_insert_resize_and_structural_undo_redo_clear_origin(self):
        self.select_origin()
        overlay = self.window.jump_reach
        self.window.doc.apply_change("Add Level", insert_level_data(self.window.map_data, 1))
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.window.undo_stack.undo()
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.window.undo_stack.redo()
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.window.doc.apply_change("Resize Map", resize_map_data(self.window.map_data, 10, 10, 0, 0))
        self.assertIsNone(overlay.origin)

    def test_same_size_replacement_and_nested_switch_clear_origin(self):
        self.select_origin()
        overlay = self.window.jump_reach
        self.window.doc.load(self.path)
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_geometry"] = {"platform": empty_map(8, 8)}
        self.window.doc.replace_with_new(data)
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.window.doc.select_map("platform")
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.assertTrue(overlay.results)
        self.window.doc.select_map(None)
        self.assertIsNone(overlay.origin)

    def test_settings_reload_updates_results_and_invalid_settings_clear_them(self):
        self.select_origin()
        overlay = self.window.jump_reach
        settings = load_map_settings("hotel")
        settings["movement"]["player"]["run_speed"] = 0.1
        with patch("map_editor.jump_reach_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        self.assertFalse(overlay.results.get((0, 5, 2), 0) & NORMAL)
        settings["movement"]["player"]["run_speed"] = None
        with patch("map_editor.jump_reach_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        self.assertEqual(overlay.results, {})
        self.assertIn("movement.player.run_speed", overlay.legend.text())
        self.window.reload_dependencies()
        self.assertTrue(overlay.results)
        self.assertIsNone(overlay.error)

    def test_hover_reports_combinations_and_origin(self):
        self.select_origin()
        overlay = self.window.jump_reach
        self.assertEqual(overlay.hover_text(2, 2), "Jump Reach origin")
        self.assertEqual(overlay.hover_text(3, 2), "Jump Reach: Normal, Speed, Anti-gravity, Both")
        self.window.mode_combo.setCurrentText(MODE_FLOOR_MATERIAL)
        canvas = self.window.canvas
        canvas._update_material_hover(QPointF(1.5 * canvas.cell_size(), 1.5 * canvas.cell_size()))
        self.assertIn("Floor", canvas._hover_label.text())
        self.assertIn("Jump Reach:", canvas._hover_label.text())
        overlay.clear_button.click()
        self.assertFalse(canvas._hover_label.isVisible())
        self.assertIsNone(overlay.hover_text(3, 2))

    def test_cancelled_click_keeps_the_origin(self):
        self.select_origin()
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton)
        QTest.keyClick(canvas, Qt.Key.Key_Escape)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton)
        self.assertEqual(self.window.jump_reach.origin, (0, 2, 2))

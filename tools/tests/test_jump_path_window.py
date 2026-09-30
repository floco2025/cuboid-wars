import copy
import unittest
from unittest.mock import patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest

from config_fixtures import gameplay, map_settings
from editor_fixtures import WindowTestCase, floor
from map_editor import jump_path_overlay
from map_editor.constants import MODE_FLOOR, MODE_JUMP_PATH, MODE_SELECT
from map_editor.jump_path import AFTER_EXIT, BEFORE_ENTRY, Takeoff
from map_editor.jump_settings import NORMAL, SCENARIO_BITS, SPEED
from map_editor.normalization import empty_map
from map_editor.portal_surfaces import PortalSurface
from map_editor.transforms import insert_level_data, resize_map_offset


class JumpPathWindowTests(WindowTestCase):
    def input(self, key):
        combo = self.window.jump_path.input_selector
        combo.setCurrentIndex(combo.findData(key))
        self.app.processEvents()

    # Two storeys; the takeoff heads east off a planned floor at (2, 2) upstairs,
    # viewed from below. The margin brings the normal landing down mid-cell,
    # clear of the grid lines where a click picks a wall.
    def start(self):
        self.window.doc.replace_with_new(insert_level_data(self.window.map_data, 1))
        self.window.set_mode(MODE_JUMP_PATH)
        self.window.jump_path.margin.setValue(0.25)
        self.window.select_level(1)
        self.app.processEvents()
        self.click_at(2.9, 2.5)
        self.window.select_level(0)
        self.app.processEvents()
        return self.window.jump_path

    # Where a scenario of the free flight comes down on a level, in grid units.
    def landing(self, level=0, bit=NORMAL):
        overlay = self.window.jump_path
        flight = next(flight for flight in overlay.free if flight.bit == bit)
        crossing = next(crossing for crossing in flight.crossings if crossing.level == level)
        return tuple(value / overlay.settings.cell_size for value in crossing.point)

    # Portal 1 under the normal landing downstairs and portal 2 on the east face of a wall line further on.
    def pair(self):
        overlay = self.start()
        self.input("entry_floor")
        self.click_at(*self.landing())
        self.input("exit_wall")
        self.click_at(6.03, 5.5)
        return overlay

    def hover(self, x, z):
        canvas = self.window.canvas
        canvas._update_cell_hover(QPointF(x * canvas.cell_size(), z * canvas.cell_size()))
        return canvas._hover_label.text() if canvas._hover_label.isVisible() else ""

    def test_a_click_near_an_edge_sets_the_takeoff_without_editing_the_document(self):
        self.window.doc.replace_with_new(insert_level_data(self.window.map_data, 1))
        before = copy.deepcopy(self.window.doc.root_data)
        count, dirty = self.window.undo_stack.count(), self.window.dirty
        self.window.set_mode(MODE_JUMP_PATH)
        overlay = self.window.jump_path
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertIn("set the takeoff", overlay.legend.text())
        self.window.select_level(1)
        self.click_at(2.9, 2.5)
        takeoff = overlay.takeoff
        self.assertEqual((takeoff.level, takeoff.col, takeoff.row, takeoff.side), (1, 2, 2, "E"))
        self.assertAlmostEqual(takeoff.along, 0.5, places=1)
        self.assertEqual(tuple(flight.bit for flight in overlay.free), SCENARIO_BITS)
        self.assertIsNone(overlay.through)
        self.assertEqual(overlay.input_selector.currentData(), "takeoff")
        # Another click replaces it; one just past a ledge means the ledge.
        self.window.select_level(0)
        self.click_at(2.05, 1.5)
        self.assertEqual(overlay.takeoff, Takeoff(0, 1, 1, "E", overlay.takeoff.along))
        self.assertEqual(self.window.doc.root_data, before)
        self.assertEqual(self.window.undo_stack.count(), count)
        self.assertEqual(self.window.dirty, dirty)

    def test_jump_step_margin_and_air_control_recompute(self):
        self.assertAlmostEqual(self.window.jump_path.margin.value(), 0.1)
        overlay = self.start()
        self.assertTrue(overlay.controls.isVisible())
        self.assertIs(overlay.controls.parentWidget(), self.window.map_combo.parentWidget())
        self.assertEqual(overlay.kind.currentText(), "Jump")
        self.assertFalse(overlay.air_control.isChecked())
        edge = overlay.takeoff.point(overlay.settings, overlay.footprints)
        jump = overlay.free
        # Hotel runs 9 m/s, so a jump 0.25 s early leaves the ground 2.25 m before the edge.
        self.assertAlmostEqual(jump[0].path[0][0], edge[0] - 2.25, places=3)
        overlay.margin.setValue(0.2)
        self.assertAlmostEqual(overlay.free[0].path[0][0], edge[0] - 1.8, places=3)
        overlay.kind.setCurrentText("Step")
        self.assertFalse(overlay.margin.isEnabled())
        self.assertAlmostEqual(overlay.free[0].path[0][0], edge[0], places=3)
        self.assertLess(self.landing()[0], jump[0].crossings[-1].point[0] / overlay.settings.cell_size)
        self.assertFalse(overlay.free[0].range)
        self.assertIn("Step / Air control off", overlay.legend.text())
        overlay.air_control.setChecked(True)
        self.assertTrue(overlay.free[0].range[0] and overlay.free[0].capture_steered[0])
        self.assertIn("Air control on", overlay.legend.text())
        self.assertTrue(any(region.kind == "range" for region in overlay.regions(0)))
        self.window.set_mode(MODE_SELECT)
        self.app.processEvents()
        self.assertFalse(overlay.controls.isVisible())
        self.assertTrue(overlay.toolbar.isVisible())

    def test_portal_1_under_the_landing_is_entered_and_portal_2_continues_the_flight(self):
        overlay = self.start()
        before = copy.deepcopy(self.window.doc.root_data)
        self.assertIn("the outline catches a floor portal", overlay.legend.text())
        self.assertTrue(any(region.kind == "capture" for region in overlay.regions(0)))
        self.input("entry_floor")
        x, z = self.landing()
        self.click_at(x, z)
        self.assertEqual((overlay.entry.face, overlay.entry.col, overlay.entry.row), ("floor", int(x), int(z)))
        self.assertAlmostEqual(overlay.entry.grid_center[0], x, places=1)
        normal = overlay.through[0]
        self.assertIn(normal.entry, ("direct", "funnel"))
        self.assertEqual(normal.end, "entered")
        self.assertEqual(normal.hop, len(normal.path) - 1)
        self.assertRegex(overlay.legend.text(), "Into portal 1: (enters|drawn in)")
        # The capture outlines stay while portal 1 is the input being placed.
        self.assertTrue(any(region.kind == "capture" for region in overlay.regions(0)))
        self.input("exit_wall")
        self.assertFalse(overlay.regions(0))
        self.click_at(6.03, 5.5)
        self.assertEqual((overlay.exit.face, overlay.exit.col, overlay.exit.row), ("east", 6, 5))
        normal = overlay.through[0]
        self.assertEqual(normal.path[normal.hop + 1][0] // overlay.settings.cell_size, 6)
        self.assertTrue(any(crossing.phase == AFTER_EXIT for crossing in normal.crossings))
        self.assertFalse(any(c.level == 0 and c.phase == BEFORE_ENTRY for c in normal.crossings))
        self.assertIn("Through portals 1 and 2: ", overlay.legend.text())
        self.assertIs(overlay.flight, overlay.through[0])
        self.assertTrue(any(glyph.phase == AFTER_EXIT for glyph in overlay.view(0).glyphs))
        self.assertEqual(overlay.input_selector.currentData(), "exit_wall")
        self.assertEqual(self.window.doc.root_data, before)

    def test_a_missed_portal_shows_the_free_flight_and_says_so(self):
        overlay = self.start()
        self.input("entry_floor")
        self.click_at(1.5, 6.5)
        entry = overlay.entry
        self.assertEqual({flight.entry for flight in overlay.through}, {"missed"})
        self.assertEqual([flight.path for flight in overlay.through], [flight.path for flight in overlay.free])
        self.assertIn("Misses portal 1", overlay.legend.text())
        # Moving the takeoff keeps the portal and tries it again.
        self.input("takeoff")
        self.window.select_level(1)
        self.click_at(1.5, 5.1)
        self.assertEqual(overlay.takeoff.side, "N")
        self.assertEqual(overlay.entry.grid_center, entry.grid_center)
        self.assertIsNotNone(overlay.through)

    def test_unavailable_and_overlapping_portals_are_refused_and_later_ones_explained(self):
        overlay = self.start()
        data = copy.deepcopy(self.window.map_data)
        data["levels"][0]["floors"].append(dict(floor(1, 5), top="unknown"))
        self.window.doc.apply_change("Resistant floor", data)
        x, z = self.landing()
        with patch.object(self.window, "notify") as notify:
            self.input("entry_floor")
            self.click(1, 5)
            self.assertIsNone(overlay.entry)
            notify.assert_called_once_with("Floor material does not allow portals")
            self.click_at(x, z)
            self.input("exit_floor")
            self.click_at(x + 0.1, z)
            self.assertIsNone(overlay.exit)
            notify.assert_called_with("Portal 1 and portal 2 overlap")
        entry = overlay.entry
        self.assertIsNotNone(overlay.through)
        # A later edit that spoils the surface keeps the input and explains itself.
        data = copy.deepcopy(self.window.map_data)
        data["levels"][0]["floors"].append(dict(floor(entry.col, entry.row), top="unknown"))
        self.window.doc.apply_change("Block entry", data)
        self.assertEqual(overlay.entry, entry)
        self.assertIsNone(overlay.through)
        self.assertIs(overlay.flight, overlay.free[0])
        self.assertIn("Portal 1: Floor material does not allow portals", overlay.legend.text())
        self.window.undo_stack.undo()
        self.assertIsNotNone(overlay.through)

    def test_shooting_positions_turn_floor_portals_and_reset_to_the_takeoff(self):
        overlay = self.pair()
        self.assertFalse(overlay.use_takeoff_button.isVisible())
        # Shot from the takeoff, west of it, portal 1 lies east-west.
        self.assertEqual(overlay.entry.turn, 1)
        through = overlay.through
        self.input("entry_shot")
        self.assertTrue(overlay.use_takeoff_button.isVisible())
        self.assertFalse(overlay.use_takeoff_button.isEnabled())
        self.assertIn("(the takeoff)", overlay.legend.text())
        x, z = overlay.entry.grid_center
        self.click_at(x, z - 2)
        self.assertEqual(overlay.entry_shot[0], 0)
        self.assertAlmostEqual(overlay.entry_shot[2], z - 2, places=1)
        self.assertEqual(overlay.entry.turn, 2)
        self.assertEqual(overlay.exit_shot, None)
        self.assertIsNot(overlay.through, through)
        self.assertTrue(overlay.use_takeoff_button.isEnabled())
        overlay.use_takeoff_button.click()
        self.assertIsNone(overlay.entry_shot)
        self.assertEqual(overlay.entry.turn, 1)
        # A wall portal faces out of its wall wherever it is shot from; a floor portal turns.
        wall = overlay.exit
        self.input("exit_shot")
        self.click_at(6.5, 7.5)
        self.assertEqual(overlay.exit, wall)
        self.input("exit_floor")
        self.click_at(6.5, 5.5)
        self.assertEqual(overlay.exit.turn, 0)
        self.input("exit_shot")
        overlay.use_takeoff_button.click()
        self.assertEqual(overlay.exit.turn, 1)
        self.assertEqual(overlay.entry.turn, 1)

    def test_the_overlay_survives_tools_levels_edits_undo_and_escape(self):
        overlay = self.pair()
        inputs = overlay.takeoff, overlay.entry, overlay.exit
        self.window.set_mode(MODE_FLOOR)
        self.app.processEvents()
        self.click(6, 1)
        self.window.undo_stack.undo()
        self.window.undo_stack.redo()
        self.window.select_level(1)
        self.window.canvas.setFocus()
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Escape)
        self.assertEqual((overlay.takeoff, overlay.entry, overlay.exit), inputs)
        self.assertIsNotNone(overlay.through)
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertFalse(overlay.controls.isVisible())
        overlay.clear_action.trigger()
        self.assertFalse(overlay.has_selection)
        self.assertIsNone(overlay.flight)
        self.assertFalse(overlay.toolbar.isVisible())

    def test_a_floor_under_a_landing_fills_its_glyph_without_a_new_preview(self):
        overlay = self.start()
        x, z = self.landing()

        def normal():
            (glyph,) = overlay.view(0).glyphs
            return glyph

        self.assertFalse(normal().supported)
        with patch.object(jump_path_overlay, "jump_preview", wraps=jump_path_overlay.jump_preview) as preview:
            self.window.add_floor_rect((int(x), int(z)), (int(x), int(z)))
            self.assertTrue(normal().supported)
            self.window.undo_stack.undo()
            self.assertFalse(normal().supported)
            preview.assert_not_called()
            # A floor beside the takeoff joins its slab and moves the edge the flight leaves from.
            self.window.select_level(1)
            self.window.add_floor_rect((2, 2), (2, 2))
            self.window.add_floor_rect((3, 2), (3, 2))
            self.assertEqual(preview.call_count, 1)

    def test_structural_changes_clear_every_input(self):
        overlay = self.pair()
        self.window.doc.apply_change("Resize", resize_map_offset(self.window.map_data, 10, 10, 0, 0))
        self.assertFalse(overlay.has_selection)
        self.input("entry_shot")
        self.click(3, 2)
        self.assertIsNotNone(overlay.entry_shot)
        self.window.doc.apply_change("Add level", insert_level_data(self.window.map_data, 1))
        self.assertFalse(overlay.has_selection)
        self.click(3, 2)
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_geometry"] = {"platform": empty_map(8, 8)}
        self.window.doc.replace_with_new(data)
        self.assertFalse(overlay.has_selection)
        self.click(3, 2)
        self.window.doc.select_map("platform")
        self.assertFalse(overlay.has_selection)

    def test_settings_reload_recomputes_and_invalid_settings_keep_the_inputs(self):
        overlay = self.pair()
        inputs = overlay.takeoff, overlay.entry, overlay.exit
        reach = self.landing()[0]
        settings = {**gameplay(), **map_settings()}
        settings["movement"]["player"]["move_speed"] = 4
        with patch("map_editor.jump_path_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
            self.assertLess(self.landing()[0], reach)
            settings["movement"]["gravity"] = None
            self.window.reload_dependencies()
        self.assertIsNone(overlay.flight)
        self.assertIn("movement.gravity", overlay.legend.text())
        self.assertEqual((overlay.takeoff, overlay.entry, overlay.exit), inputs)
        with patch.object(self.window, "notify") as notify:
            self.input("takeoff")
            self.click(4, 4)
            notify.assert_called_once()
        self.assertEqual(overlay.takeoff, inputs[0])
        self.window.reload_dependencies()
        self.assertIsNone(overlay.error)
        self.assertAlmostEqual(self.landing()[0], reach)
        self.assertIsNotNone(overlay.through)

    def test_hover_names_landings_and_previews_the_input(self):
        overlay = self.start()
        x, z = self.landing()
        text = self.hover(x, z)
        self.assertIn("Jump Path landing", text)
        self.assertIn("● No damage", text)
        self.assertIn("No floor here", text)
        self.assertIsInstance(overlay.preview, Takeoff)
        self.assertIn("Set takeoff", text)
        self.input("entry_floor")
        self.assertIsNone(overlay.preview)
        text = self.hover(x, z)
        self.assertIsInstance(overlay.preview, PortalSurface)
        self.assertIn("Set portal 1 · Floor", text)
        self.assertIn("Planned compatible surface", text)
        self.assertIn("Set portal 1 · Floor", self.hover(5.03, 5.5))
        self.input("entry_wall")
        self.assertIn("Set portal 1 · East", self.hover(5.03, 5.5))
        self.assertIn("Set portal 1 · South", self.hover(5.4, 5.3))
        self.window.set_mode(MODE_SELECT)
        self.app.processEvents()
        text = self.hover(x, z)
        self.assertIsNone(overlay.preview)
        self.assertIn("Jump Path landing", text)
        self.assertNotIn("Set portal", text)
        overlay.clear_button.click()
        self.assertEqual(self.hover(x, z), "")

    def test_the_power_up_selector_shows_one_flight_at_a_time(self):
        overlay = self.start()
        self.assertEqual(overlay.power_ups.currentData(), NORMAL)
        normal = self.landing()
        (glyph,) = overlay.view(0).glyphs
        self.assertAlmostEqual(glyph.point[0] / overlay.settings.cell_size, normal[0])
        with patch.object(jump_path_overlay, "jump_preview", wraps=jump_path_overlay.jump_preview) as preview:
            overlay.power_ups.setCurrentIndex(overlay.power_ups.findData(SPEED))
            preview.assert_not_called()
        self.assertEqual(overlay.flight.bit, SPEED)
        (glyph,) = overlay.view(0).glyphs
        self.assertAlmostEqual(glyph.point[0] / overlay.settings.cell_size, self.landing(bit=SPEED)[0])
        self.assertGreater(glyph.point[0] / overlay.settings.cell_size, normal[0])
        # It sits with the legend, so it stays in reach while another tool is in use.
        self.window.set_mode(MODE_SELECT)
        self.app.processEvents()
        self.assertTrue(overlay.power_ups.isVisible())
        self.assertFalse(overlay.controls.isVisible())

    def floor_pair(self, **air_rates):
        settings = {**gameplay(), **map_settings()}
        settings["movement"]["player"].update(air_rates)
        with patch("map_editor.jump_path_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        overlay = self.start()
        self.input("entry_floor")
        self.click_at(*self.landing())
        self.input("exit_floor")
        self.click_at(6.5, 5.5)
        return overlay

    def test_a_floor_exit_lands_where_the_entry_speed_carries_it(self):
        # No air rates, as in Obby: the sideways speed going into portal 1 comes out of portal 2.
        overlay = self.floor_pair(air_acceleration=0, air_deceleration=0, air_lateral_deceleration=0)
        self.assertEqual(overlay.flight.end, "below")
        self.assertRegex(overlay.legend.text(), "Through portals 1 and 2: (enters|drawn in)<br>")
        (landing,) = [glyph for glyph in overlay.view(0).glyphs if glyph.phase == AFTER_EXIT]
        exit_x, exit_z = overlay.exit.grid_center
        size = overlay.settings.cell_size
        # It left the takeoff heading east, so it lands east of portal 2.
        self.assertGreater(landing.point[0] / size, exit_x + 1)
        self.assertAlmostEqual(landing.point[1] / size, exit_z, places=1)
        self.assertIsNone(overlay.reentry())
        self.assertFalse(overlay.regions(0))

    def test_a_floor_exit_without_sideways_speed_falls_back_in_and_shows_where_steering_lands(self):
        # Braked to a stop in the air, the flight drops into portal 1 and comes straight up out of portal 2.
        overlay = self.floor_pair(air_deceleration=60)
        self.assertEqual(overlay.flight.end, "reentered")
        self.assertEqual(overlay.reentry(), (overlay.exit, 2))
        self.assertIn("then falls back into portal 2; steering lands in the dotted outline", overlay.legend.text())
        self.assertFalse(overlay.air_control.isChecked())
        self.assertEqual([region.kind for region in overlay.regions(0)], ["range"])
        self.window.canvas.grab()

    def test_a_floor_portal_can_sit_on_a_grid_corner_and_a_wall_portal_takes_the_nearest_edge(self):
        overlay = self.start()
        self.input("entry_floor")
        self.click_at(4.0, 3.0)
        self.assertEqual(overlay.entry.face, "floor")
        for actual, expected in zip(overlay.entry.grid_center, (4.0, 3.0)):
            self.assertAlmostEqual(actual, expected, places=1)
        self.input("exit_wall")
        self.click_at(6.4, 5.5)
        self.assertEqual((overlay.exit.face, overlay.exit.col, overlay.exit.row), ("east", 6, 5))

    def test_a_cancelled_click_keeps_the_takeoff(self):
        overlay = self.start()
        takeoff = overlay.takeoff
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton)
        QTest.keyClick(canvas, Qt.Key.Key_Escape)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton)
        self.assertEqual(overlay.takeoff, takeoff)

    def test_the_overlay_paints_every_stage(self):
        overlay = self.pair()
        overlay.air_control.setChecked(True)
        self.input("entry_shot")
        self.click(4, 0)
        self.input("entry_floor")
        self.hover(5.03, 5.5)
        for level in (0, 1):
            self.window.select_level(level)
            self.window.canvas.grab()
        overlay.clear_action.trigger()
        self.window.canvas.grab()


if __name__ == "__main__":
    unittest.main()

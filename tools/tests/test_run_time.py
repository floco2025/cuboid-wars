import copy
import unittest
from unittest.mock import patch

from editor_fixtures import WindowTestCase
from map_editor.catalogs import load_map_settings
from map_editor.constants import MODE_FLOOR, MODE_RUN_TIME
from map_editor.normalization import empty_map
from map_editor.run_time import RunSettings
from map_editor.transforms import resize_map_data


class RunTimeTests(unittest.TestCase):
    settings = RunSettings(2, 4, 1.5)

    def test_seconds_are_centre_to_centre_at_both_speeds(self):
        self.assertEqual(self.settings.seconds((1, 1), (4, 5)), (2.5, 2.5 / 1.5))
        self.assertEqual(self.settings.seconds((1, 1), (1, 1)), (0, 0))

    def test_invalid_fields_name_the_source_and_field(self):
        settings = {"geometry": {"grid_cell_size": 2}, "movement": {"player": {"run_speed": 4, "speed_power_up": 1.5}}}
        self.assertEqual(RunSettings.from_settings(settings, "settings.json"), self.settings)
        for path in ("geometry.grid_cell_size", "movement.player.run_speed", "movement.player.speed_power_up"):
            with self.subTest(path=path):
                invalid = copy.deepcopy(settings)
                section, *keys = path.split(".")
                value = invalid[section]
                for key in keys[:-1]:
                    value = value[key]
                value[keys[-1]] = 0
                with self.assertRaisesRegex(ValueError, f"settings.json: {path}"):
                    RunSettings.from_settings(invalid, "settings.json")


class RunTimeWindowTests(WindowTestCase):
    def select_origin(self, col=2, row=2):
        self.window.mode_combo.setCurrentText(MODE_RUN_TIME)
        self.app.processEvents()
        self.click(col, row)

    def test_click_sets_origin_hover_reports_seconds_and_clear_removes_them(self):
        before = copy.deepcopy(self.window.doc.root_data)
        self.select_origin()
        overlay = self.window.run_time
        self.assertEqual(overlay.origin, (0, 2, 2))
        self.assertEqual(overlay.hover_text(2, 2), "Run Time origin")
        self.assertEqual(overlay.hover_text(5, 2), "Run Time: 1.13 s run, 0.76 s with speed")
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertIn("Run 9.0 m/s", overlay.legend.text())
        self.assertIn("Run + Speed 13.5 m/s", overlay.legend.text())
        self.assertEqual(self.window.doc.root_data, before)
        self.assertFalse(self.window.dirty)
        self.click(4, 3)
        self.assertEqual(overlay.origin, (0, 4, 3))
        overlay.clear_action.trigger()
        self.assertIsNone(overlay.origin)
        self.assertIsNone(overlay.hover_text(5, 2))
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertFalse(overlay.clear_button.isVisible())
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.app.processEvents()
        self.assertFalse(overlay.toolbar.isVisible())

    def test_floor_edits_keep_the_origin_while_resizing_and_switching_maps_clear_it(self):
        self.select_origin()
        overlay = self.window.run_time
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.app.processEvents()
        self.click(4, 4)
        self.assertEqual(overlay.origin, (0, 2, 2))
        self.window.doc.apply_change("Resize Map", resize_map_data(self.window.map_data, 10, 10, 0, 0))
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_geometry"] = {"platform": empty_map(8, 8)}
        self.window.doc.replace_with_new(data)
        self.assertIsNone(overlay.origin)
        overlay.select(2, 2)
        self.window.doc.select_map("platform")
        self.assertIsNone(overlay.origin)

    def test_settings_reload_updates_speeds_and_invalid_settings_disable_the_tool(self):
        self.select_origin()
        overlay = self.window.run_time
        settings = load_map_settings("hotel")
        settings["movement"]["player"]["run_speed"] = 18
        with patch("map_editor.run_time_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        self.assertEqual(overlay.hover_text(5, 2), "Run Time: 0.57 s run, 0.38 s with speed")
        settings["movement"]["player"]["run_speed"] = None
        with patch("map_editor.run_time_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        self.assertIsNone(overlay.hover_text(5, 2))
        self.assertIn("movement.player.run_speed", overlay.legend.text())
        self.window.reload_dependencies()
        self.assertIsNone(overlay.error)
        self.assertEqual(overlay.hover_text(5, 2), "Run Time: 1.13 s run, 0.76 s with speed")

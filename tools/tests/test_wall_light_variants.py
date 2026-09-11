import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QPointF
from PySide6.QtWidgets import QMessageBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.constants import HIT_LIGHT, MODE_LIGHT
from map_editor.hover import element_hover_text
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map
from map_editor.validation import validate_map


class WallLightValidationTests(unittest.TestCase):
    def test_unknown_style_is_reported_and_preserved_for_repair(self):
        data = empty_map(8, 8)
        data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
        data["levels"][0]["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "all": DEFAULT_ALIAS}]
        data["levels"][0]["lights"] = [{"col": 1, "row": 1, "side": "N", "kind": "unknown"}]
        errors = validate_map(data, [], [], wall_light_kinds=["decorative", "utility"])
        self.assertTrue(any("unknown kind 'unknown'" in error for error in errors))
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "layout.json"
            write_map(path, data)
            self.assertEqual(read_map(path)["levels"][0]["lights"][0]["kind"], "unknown")


class WallLightVariantTests(WindowTestCase):
    def test_selected_variant_survives_save_reload_and_undo(self):
        window = self.window
        window.add_wall_line((1, 1), (2, 1))
        window.set_mode(MODE_LIGHT)
        selector = next(
            widget for widget, attribute in window.tool_settings.bindings if attribute == "recent_light_kind"
        )
        selector.setCurrentText("utility")
        window.add_light_at(QPointF(1.5, 1.05))
        self.assertEqual(window.map_data["levels"][0]["lights"][0]["kind"], "utility")
        self.assertIn("utility", element_hover_text(window.map_data, 0, (HIT_LIGHT, (1, 1, "N"))))
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "layout.json"
            write_map(path, window.map_data)
            restored = read_map(path)
            self.assertEqual(restored["levels"][0]["lights"], window.map_data["levels"][0]["lights"])
        window.undo_stack.undo()
        self.assertEqual(window.map_data["levels"][0]["lights"], [])
        window.undo_stack.redo()
        self.assertEqual(window.map_data["levels"][0]["lights"][0]["kind"], "utility")

    def test_auto_place_uses_dialog_style_and_keeps_existing_lights(self):
        window = self.window
        window.add_floor_rect((1, 1), (2, 1))
        window.add_wall_line((1, 1), (3, 1))
        window.recent_light_kind = "decorative"
        window.add_light_at(QPointF(1.5, 1.05))
        with (
            patch(
                "map_editor.lights.AutoPlaceLightsDialog.prompt",
                return_value=(0, 0, 0, 0, "utility"),
            ),
            patch(
                "map_editor.lights.QMessageBox.question",
                return_value=QMessageBox.StandardButton.Yes,
            ),
        ):
            window.open_auto_place_lights_dialog()
        self.assertEqual(
            {light["col"]: light["kind"] for light in window.map_data["levels"][0]["lights"]},
            {1: "decorative", 2: "utility"},
        )
        window.undo_stack.undo()
        self.assertEqual(
            [light["kind"] for light in window.map_data["levels"][0]["lights"]],
            ["decorative"],
        )

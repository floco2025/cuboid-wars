import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QPointF
from PySide6.QtWidgets import QMessageBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.constants import HIT_LIGHT, MODE_LIGHT
from map_editor.elements import element_refs
from map_editor.hover import element_hover_text
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map, normalize_map
from map_editor.validation import validate_map


class WallLightValidationTests(unittest.TestCase):
    def test_unknown_style_is_reported_and_preserved_for_repair(self):
        data = empty_map(8, 8)
        data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
        data["levels"][0]["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "all": DEFAULT_ALIAS}]
        data["levels"][0]["lights"] = [{"col": 1, "row": 1, "side": "N", "kind": "unknown"}]
        errors = validate_map(data, [], wall_light_kinds=["decorative", "utility"])
        self.assertTrue(any("unknown kind 'unknown'" in error for error in errors))
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "layout.json"
            write_map(path, data)
            self.assertEqual(read_map(path)["levels"][0]["lights"][0]["kind"], "unknown")


class WallLightHeightTests(unittest.TestCase):
    def test_a_light_needs_a_positive_height(self):
        data = empty_map(8, 8)
        data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
        data["levels"][0]["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "all": DEFAULT_ALIAS}]
        light = {"col": 1, "row": 1, "side": "N", "kind": "utility", "height": 0.3}
        data["levels"][0]["lights"] = [light]
        self.assertEqual([e for e in validate_map(data, [], wall_light_kinds=["utility"]) if "light" in e], [])
        self.assertEqual(normalize_map(data)["levels"][0]["lights"][0]["height"], 0.3)
        for height in (-1, None):
            light["height"] = height
            errors = validate_map(data, [], wall_light_kinds=["utility"])
            self.assertTrue(any("needs a positive height" in error for error in errors), (height, errors))


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
        # A map without lights hangs its first part way up a wall section.
        self.assertEqual(window.map_data["levels"][0]["lights"][0]["height"], window.recent_light_height)
        self.assertAlmostEqual(window.recent_light_height, 0.625 * (window.level_height - window.floor_thickness))
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

    def test_properties_sets_the_height_of_every_selected_light(self):
        window = self.window
        window.add_wall_line((1, 1), (4, 1))
        window.recent_light_kind = "utility"
        for x in (1.5, 2.5, 3.5):
            window.add_light_at(QPointF(x, 1.05))
        first = window.recent_light_height
        after = copy.deepcopy(window.map_data)
        after["levels"][0]["lights"][2]["height"] = 1.2
        window.apply_change("Lower", after)
        refs = [ref for ref, _ in element_refs(window.map_data) if ref.name == "lights"]
        window.inspect_refs(refs)
        panel = window.properties_panel
        self.assertEqual(panel.widgets[("height",)].placeholderText(), "Mixed / unchanged")
        self.set_property("height", 1.9)
        self.assertEqual([light["height"] for light in window.map_data["levels"][0]["lights"]], [1.9] * 3)
        self.assertIn("1.9 m up", element_hover_text(window.map_data, 0, (HIT_LIGHT, (2, 1, "N"))))
        window.undo_stack.undo()
        self.assertEqual(window.map_data["levels"][0]["lights"][2]["height"], 1.2)
        self.assertEqual(window.map_data["levels"][0]["lights"][0]["height"], first)
        # The next light follows the last one placed or sampled, so the Sample tool hands its height on.
        window.sample_ref(next(ref for ref in refs if ref.get(window.map_data)["height"] == 1.2))
        self.assertEqual(window.recent_light_height, 1.2)

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

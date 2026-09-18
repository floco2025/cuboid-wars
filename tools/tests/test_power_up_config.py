import copy
import json
from unittest.mock import patch

from config_fixtures import ConfigTestCase
from editor_fixtures import WindowTestCase, nested
from map_editor.catalogs import MapCatalogs, load_map_settings, map_settings_path, pickup_types
from map_editor.constants import ITEM_TYPES
from map_editor.dialogs import ItemTypeDialog
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map
from map_editor.property_fields import fields_for


class PowerUpCatalogTests(ConfigTestCase):
    def test_modes_control_pickups_and_conflicting_weights_are_rejected(self):
        path = map_settings_path("hotel")
        settings = load_map_settings("hotel")
        settings["power_ups"]["single_shot"] = {"mode": "always"}
        path.write_text(json.dumps(settings))
        self.assertEqual(
            MapCatalogs.load("hotel").pickup_types, tuple(kind for kind in ITEM_TYPES if kind != "single_shot")
        )
        for weight in (0, 1):
            settings["random_items"] = {"weights": {"single_shot": weight}}
            with self.assertRaisesRegex(ValueError, "single_shot is always active"):
                pickup_types(settings, "settings.json")

    def test_pickup_duration_requires_positive_seconds_or_explicit_null(self):
        settings = load_map_settings("hotel")
        for rule in (
            {"mode": "pickup"},
            {"mode": "always", "duration_secs": None},
            *({"mode": "pickup", "duration_secs": value} for value in (0, -1, True, "30", [], float("inf"))),
        ):
            settings["power_ups"]["speed"] = rule
            with self.assertRaisesRegex(ValueError, "power_ups.speed"):
                pickup_types(settings, "settings.json")
        for value in (None, 30):
            settings["power_ups"]["speed"] = {"mode": "pickup", "duration_secs": value}
            self.assertIn("speed", pickup_types(settings, "settings.json"))

    def test_optional_feature_sections_are_explicit(self):
        path = map_settings_path("hotel")
        settings = load_map_settings("hotel")
        for field in ("grounds", "random_items", "placed_items"):
            invalid = copy.deepcopy(settings)
            del invalid[field]
            path.write_text(json.dumps(invalid))
            with self.assertRaisesRegex(ValueError, field):
                load_map_settings("hotel")
            invalid[field] = []
            path.write_text(json.dumps(invalid))
            with self.assertRaisesRegex(ValueError, field):
                load_map_settings("hotel")


class AlwaysPowerUpEditorTests(WindowTestCase):
    def test_reload_updates_choices_and_rejects_always_active_placements(self):
        window = self.window
        path = map_settings_path("hotel")
        settings = load_map_settings("hotel")
        settings["power_ups"]["single_shot"] = {"mode": "always"}
        path.write_text(json.dumps(settings))
        window.adopt_map("hotel")
        self.assertNotIn("single_shot", window.pickup_types)
        self.assertNotEqual(window.recent_item_type, "single_shot")
        field = next(field for field in fields_for(window, "items") if field.key == ("type",))
        self.assertNotIn("single_shot", dict(field.choices))
        dialog = ItemTypeDialog(window, "Place Item", [], None, None, item_types=window.pickup_types)
        self.assertEqual(dialog._type_combo.findText("single_shot"), -1)
        dialog.deleteLater()
        before = copy.deepcopy(window.map_data)
        with patch.object(window, "notify") as notice:
            window.add_item(2, 2, "single_shot", None)
        self.assertEqual(window.map_data, before)
        self.assertIn("unavailable", notice.call_args.args[0])
        before["items"] = [{"level": 0, "col": 1, "row": 1, "type": "single_shot"}]
        self.assertTrue(any("always active" in error for error in window.validate_document(before)))
        before["items"] = []
        before["nested_geometry"] = {"room": empty_map(2, 2)}
        before["nested_geometry"]["room"]["items"] = [{"level": 0, "col": 1, "row": 1, "type": "single_shot"}]
        self.assertTrue(
            any("Nested room" in error and "always active" in error for error in window.validate_document(before))
        )

    def test_null_nested_destination_round_trips_as_its_inherited_level(self):
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_maps"] = [{**nested("room", 2, [0, 0], [1, 0]), "to_level": None}]
        data["nested_geometry"] = {"room": empty_map(2, 2)}
        self.path.write_text(json.dumps({"map": data}))
        loaded = read_map(self.path)
        self.assertEqual(loaded["nested_maps"][0]["to_level"], 2)
        write_map(self.path, loaded)
        saved = json.loads(self.path.read_text())["map"]
        self.assertIsNone(saved["fireworks"])
        self.assertNotIn("fireworks", saved["nested_geometry"]["room"])
        self.assertEqual(saved["nested_maps"][0]["to_level"], 2)
        self.assertEqual(read_map(self.path), loaded)

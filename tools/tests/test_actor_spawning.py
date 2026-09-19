from map_editor.constants import MODE_SELECT
import copy
import unittest

from PySide6.QtWidgets import QDialogButtonBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.catalogs import MapCatalogs
from map_editor.io import read_map
from map_editor.dialogs import ActorSpawnFieldsDialog
from map_editor.constants import MODE_ACTOR_SPAWN_ZONE
from map_editor.normalization import canonicalize_map, empty_level, empty_map
from map_editor.geometry import roam_slice_radius, zone_spans_level
from map_editor.transforms import insert_level_data, remove_level_data, record_levels
from map_editor.validation import validate_document, validate_map

ACTOR_KINDS = ["turret", "scuttler", "zapper"]


def spawn_map(kind="turret"):
    data = empty_map(4, 2)
    data["checkpoints"] = []
    data["actor_spawn_zones"] = [
        {"level": 0, "cols": [0, 4], "rows": [0, 2], "kind": kind, "count": [100], "respawn_secs": 90}
    ]
    data["levels"][0]["floors"] = []
    return data


class SpawnValidationTests(unittest.TestCase):
    def test_count_lists_round_trip_canonical_order_and_keep_invalid_values_for_validation(self):
        data = spawn_map()
        zone = data["actor_spawn_zones"][0]
        data["actor_spawn_zones"] = [
            {**zone, "count": count} for count in [[1, 3, 4], [3], [1], [0, 2], [1], [1, 3, 4]]
        ]
        normalized = canonicalize_map(data)
        self.assertEqual([z["count"] for z in normalized["actor_spawn_zones"]], [[0, 2], [1], [1, 3, 4], [3]])
        self.assertEqual(self.validate(normalized), [])
        self.assertEqual(canonicalize_map(normalized), normalized)
        for count in [3, [], [2, 1], [0, -1], [1, 2.5], [False], "3", None, {}, 2**32]:
            with self.subTest(count=count):
                invalid = spawn_map()
                invalid["actor_spawn_zones"][0]["count"] = count
                normalized = canonicalize_map(invalid)
                self.assertEqual(normalized["actor_spawn_zones"][0]["count"], count)
                self.assertTrue(any("Count" in error or "Counts" in error for error in self.validate(normalized)))

    def test_volume_and_extension_survive_level_insertions_and_removals_as_one_zone(self):
        data = spawn_map()
        data["levels"].extend([empty_level(1), empty_level(2)])
        zone = data["actor_spawn_zones"][0]
        zone.update(levels=3, roam_distance=2.5, count=[1])
        self.assertEqual(self.validate(data), [])
        self.assertEqual(record_levels(zone), (0, 2))
        self.assertTrue(zone_spans_level(zone, 2))
        inserted = insert_level_data(data, 1)
        self.assertEqual(inserted["actor_spawn_zones"][0]["levels"], 4)
        restored = remove_level_data(inserted, 1)
        self.assertEqual(restored["actor_spawn_zones"], data["actor_spawn_zones"])
        shrunk = remove_level_data(restored, 0)
        self.assertEqual(len(shrunk["actor_spawn_zones"]), 1)
        self.assertEqual(shrunk["actor_spawn_zones"][0]["levels"], 2)
        self.assertEqual(shrunk["actor_spawn_zones"][0]["count"], [1])

    def test_extensions_and_level_spans_are_validated(self):
        for distance in [-1, float("nan"), float("inf"), "far"]:
            data = spawn_map()
            data["actor_spawn_zones"][0]["roam_distance"] = distance
            self.assertTrue(self.validate(data))
        for levels in [0, 2, 1.5, "two"]:
            data = spawn_map()
            data["actor_spawn_zones"][0]["levels"] = levels
            self.assertTrue(self.validate(data))

    def test_roam_slice_uses_euclidean_distance_above_and_below_the_volume(self):
        zone = {"level": 1, "levels": 2, "roam_distance": 5.0}
        self.assertEqual(roam_slice_radius(zone, 1, 4.0), 5.0)
        self.assertEqual(roam_slice_radius(zone, 2, 4.0), 5.0)
        self.assertEqual(roam_slice_radius(zone, 0, 4.0), 3.0)
        self.assertEqual(roam_slice_radius(zone, 4, 4.0), 3.0)
        self.assertIsNone(roam_slice_radius(zone, 5, 4.0))
        self.assertIsNone(roam_slice_radius({**zone, "roam_distance": 0}, 1, 4.0))

    def validate(self, data):
        return validate_map(data, [], ["green"], actor_kinds=ACTOR_KINDS)

    def test_all_spawn_kinds_allow_any_surface_without_floor_capacity_limits(self):
        for kind in ACTOR_KINDS:
            for surface in ("air", "floor", "inaccessible_floor", "bridge", "ramp"):
                with self.subTest(kind=kind, surface=surface):
                    data = spawn_map(kind)
                    level = data["levels"][0]
                    if surface == "floor":
                        level["floors"] = [{"col": 0, "row": 0, "all": DEFAULT_ALIAS}]
                    elif surface == "inaccessible_floor":
                        level["inaccessible_floors"] = [{"col": 0, "row": 0, "all": DEFAULT_ALIAS}]
                    elif surface == "bridge":
                        level["light_bridges"] = [{"col": 0, "row": 0, "kind": "green"}]
                    elif surface == "ramp":
                        data["levels"].append(empty_level(1))
                        data["ramps"] = [
                            {"lower_level": 0, "cols": [1, 2], "rows": [0, 2], "direction": "S", "all": DEFAULT_ALIAS}
                        ]
                    self.assertEqual(self.validate(data), [])

    def test_invalid_level_is_reported_without_reading_a_missing_floor_list(self):
        data = spawn_map()
        data["actor_spawn_zones"][0]["level"] = 5
        self.assertTrue(any("invalid level" in error for error in self.validate(data)))

    def test_nested_spawn_zones_allow_bridges_without_ground_support(self):
        data = {"fireworks": None, **empty_map(8, 8)}
        data["levels"][0]["floors"] = [{"col": c, "row": r, "all": DEFAULT_ALIAS} for c in range(2) for r in range(2)]
        child = spawn_map()
        child["levels"][0]["light_bridges"] = [{"col": 0, "row": 0, "kind": "green"}]
        data["nested_geometry"] = {"turret_room": child}
        errors = validate_document(
            data,
            MapCatalogs({}, {"green": "#00ff00"}, 0.1, {DEFAULT_ALIAS: True}),
            actor_kinds=ACTOR_KINDS,
        )
        self.assertEqual(errors, [])


class SpawnWindowTests(WindowTestCase):
    def test_count_control_edits_lists_and_rejects_invalid_input(self):
        dialog = ActorSpawnFieldsDialog(self.window, "zapper", [0, 2, 4], None, 3.0, [], None)
        control = dialog.count_control
        self.assertEqual(dialog.values()[1], [0, 2, 4])
        control.counts.setText("2, 1")
        buttons = dialog.findChild(QDialogButtonBox)
        self.assertFalse(buttons.button(QDialogButtonBox.StandardButton.Ok).isEnabled())
        control.counts.setText("1, 3, 3, 5")
        self.assertTrue(buttons.button(QDialogButtonBox.StandardButton.Ok).isEnabled())
        self.assertEqual(dialog.values()[1], [1, 3, 3, 5])
        control.counts.setText("7")
        self.assertEqual(dialog.values()[1], [7])
        dialog.deleteLater()

    def test_course_control_reports_the_closing_checkpoint_and_response(self):
        dialog = ActorSpawnFieldsDialog(
            self.window, "zapper", [1], None, 3.0, [], None, until_checkpoint=4, on_checkpoint="destroy"
        )
        self.assertEqual(dialog.values()[9:], (4, "destroy"))
        self.assertTrue(dialog.course.response.isEnabled())
        dialog.course.until.setValue(0)
        self.assertEqual(dialog.values()[9:], (None, None))
        self.assertFalse(dialog.course.response.isEnabled())
        dialog.deleteLater()

    def test_zones_ending_at_a_checkpoint_are_painted_saved_and_reloaded(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["checkpoints"].append({"level": 0, "cols": [1, 2], "rows": [1, 2], "type": "individual", "number": 2})
        window.apply_change("Add checkpoint", data)
        window.recent_actor_spawn_kind = "zapper"
        window.recent_actor_until_checkpoint = 2
        window.recent_actor_on_checkpoint = "destroy"
        window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        zone = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["until_checkpoint"], zone["on_checkpoint"]), (2, "destroy"))
        self.assertEqual(window.validate_document(window.doc.root_data), [])
        self.assertTrue(window.save())
        self.assertEqual(read_map(self.path)["actor_spawn_zones"], window.map_data["actor_spawn_zones"])

    def test_scaled_zones_can_be_painted_edited_undone_saved_and_reloaded(self):
        window = self.window
        window.recent_actor_spawn_count = [0, 2, 3]
        window.recent_actor_spawn_kind = "zapper"
        window.set_mode(MODE_ACTOR_SPAWN_ZONE)
        window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [0, 2, 3])
        window.tool_settings.refresh()
        self.assertIn("3+ players → 3 actors", window.tool_settings.actor_count.toolTip())
        window.set_mode(MODE_SELECT)
        self.click(2, 2)
        self.set_property("count", "1, 2, 4, 6")
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [1, 2, 4, 6])
        window.undo_stack.undo()
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [0, 2, 3])
        window.undo_stack.redo()
        zones = copy.deepcopy(window.map_data["actor_spawn_zones"])
        self.assertTrue(window.save())
        self.assertEqual(read_map(self.path)["actor_spawn_zones"], zones)
        window.load_path(self.path)
        window.reload_dependencies()
        self.assertEqual(window.map_data["actor_spawn_zones"], zones)
        self.assertEqual(window.validate_document(window.doc.root_data), [])

    def test_multilevel_spawn_zones_save_one_quota_and_their_roam_extension(self):
        window = self.window
        data = insert_level_data(window.map_data, 1)
        window.apply_change("Add level", data)
        window.recent_actor_spawn_levels = 2
        window.recent_actor_roam_distance = 3.5
        window.recent_actor_spawn_count = [1]
        window.recent_actor_spawn_kind = "zapper"
        window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        self.assertTrue(window.save())
        saved = read_map(self.path)
        zone = saved["actor_spawn_zones"][0]
        self.assertEqual((zone["levels"], zone["count"], zone["roam_distance"]), (2, [1], 3.5))

    def test_unsupported_spawn_zones_can_be_created_saved_and_reloaded(self):
        window = self.window
        window.recent_actor_spawn_count = [100]
        for kind in ACTOR_KINDS:
            window.recent_actor_spawn_kind = kind
            window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        zones = window.map_data["actor_spawn_zones"]
        self.assertEqual(len(zones), len(ACTOR_KINDS))
        self.assertEqual(window.validate_document(window.doc.root_data), [])
        self.assertTrue(window.save())
        saved = read_map(self.path)
        self.assertEqual(saved["actor_spawn_zones"], zones)
        window.load_path(self.path)
        window.reload_dependencies()
        self.assertEqual(window.map_data["actor_spawn_zones"], zones)
        self.assertEqual(window.validate_document(window.doc.root_data), [])

import unittest

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.catalogs import MapCatalogs
from map_editor.io import read_map
from map_editor.normalization import empty_level, empty_map
from map_editor.geometry import roam_slice_radius, zone_spans_level
from map_editor.transforms import insert_level_data, remove_level_data, record_levels
from map_editor.validation import validate_document, validate_map

ACTOR_KINDS = ["turret", "scuttler", "zapper"]


def spawn_map(kind="turret"):
    data = empty_map(4, 2)
    data["player_spawn_zones"] = [{"level": 0, "cols": [0, 4], "rows": [0, 2]}]
    data["actor_spawn_zones"] = [
        {"level": 0, "cols": [0, 4], "rows": [0, 2], "kind": kind, "count": 100, "respawn_secs": 90}
    ]
    data["levels"][0]["floors"] = []
    return data


class SpawnValidationTests(unittest.TestCase):
    def test_volume_and_extension_survive_level_insertions_and_removals_as_one_zone(self):
        data = spawn_map()
        data["levels"].extend([empty_level(1), empty_level(2)])
        zone = data["actor_spawn_zones"][0]
        zone.update(levels=3, roam_distance=2.5, count=1)
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
        self.assertEqual(shrunk["actor_spawn_zones"][0]["count"], 1)

    def test_extensions_and_level_spans_are_validated(self):
        for distance in [-1, float("nan"), float("inf"), "far"]:
            data = spawn_map()
            data["actor_spawn_zones"][0]["roam_distance"] = distance
            self.assertTrue(self.validate(data))
        for levels in [0, 2, 1.5, "two"]:
            for name in ["actor_spawn_zones", "player_spawn_zones"]:
                data = spawn_map()
                data[name][0]["levels"] = levels
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
                        data["ramps"] = [{"lower_level": 0, "low": [1, 0], "high": [2, 2], "all": DEFAULT_ALIAS}]
                    self.assertEqual(self.validate(data), [])

    def test_invalid_level_is_reported_without_reading_a_missing_floor_list(self):
        data = spawn_map()
        data["actor_spawn_zones"][0]["level"] = 5
        self.assertTrue(any("invalid level" in error for error in self.validate(data)))

    def test_nested_spawn_zones_allow_bridges_without_ground_support(self):
        data = empty_map(8, 8)
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
    def test_multilevel_spawn_zones_save_one_quota_and_their_roam_extension(self):
        window = self.window
        data = insert_level_data(window.map_data, 1)
        window.apply_change("Add level", data)
        window.recent_actor_spawn_levels = 2
        window.recent_actor_roam_distance = 3.5
        window.recent_actor_spawn_count = 1
        window.recent_actor_spawn_kind = "zapper"
        window.recent_player_spawn_levels = 2
        window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        window.add_player_spawn_zone_rect((2, 2), (3, 3))
        self.assertTrue(window.save())
        saved = read_map(self.path)
        zone = saved["actor_spawn_zones"][0]
        self.assertEqual((zone["levels"], zone["count"], zone["roam_distance"]), (2, 1, 3.5))
        self.assertEqual(saved["player_spawn_zones"][0]["levels"], 2)

    def test_unsupported_spawn_zones_can_be_created_saved_and_reloaded(self):
        window = self.window
        window.recent_actor_spawn_count = 100
        for kind in ACTOR_KINDS:
            window.recent_actor_spawn_kind = kind
            window.add_actor_spawn_zone_rect((2, 2), (3, 3))
        window.add_player_spawn_zone_rect((2, 2), (3, 3))
        zones = window.map_data["actor_spawn_zones"]
        players = window.map_data["player_spawn_zones"]
        self.assertEqual(len(zones), len(ACTOR_KINDS))
        self.assertEqual(window.validate_document(window.doc.root_data), [])
        self.assertTrue(window.save())
        saved = read_map(self.path)
        self.assertEqual(saved["actor_spawn_zones"], zones)
        self.assertEqual(saved["player_spawn_zones"], players)
        window.load_path(self.path)
        window.reload_dependencies()
        self.assertEqual(window.map_data["actor_spawn_zones"], zones)
        self.assertEqual(window.map_data["player_spawn_zones"], players)
        self.assertEqual(window.validate_document(window.doc.root_data), [])

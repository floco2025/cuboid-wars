import unittest
from unittest.mock import patch

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.catalogs import MapCatalogs
from map_editor.normalization import empty_map
from map_editor.validation import validate_document, validate_map


def spawn_map(count=2):
    data = empty_map(4, 2)
    data["player_spawn_zones"] = []
    data["actor_spawn_zones"] = [
        {"level": 0, "cols": [0, 4], "rows": [0, 2], "kind": "turret", "count": count}
    ]
    data["levels"][0]["floors"] = [
        {"col": col, "row": 0, "all": DEFAULT_ALIAS} for col in (0, 1)
    ]
    data["levels"][0]["inaccessible_floors"] = [{"col": 2, "row": 0, "all": DEFAULT_ALIAS}]
    data["levels"][0]["light_bridges"] = [{"col": 3, "row": 0, "kind": "green"}]
    return data


class SpawnValidationTests(unittest.TestCase):
    def validate(self, data):
        return validate_map(data, [], ["green"], actor_kinds=["turret", "scuttler"], immovable_actor_kinds={"turret"})

    def test_capacity_excludes_empty_cells_blocked_floors_and_bridges(self):
        data = spawn_map()
        self.assertFalse(self.validate(data))
        data["actor_spawn_zones"][0]["count"] = 3
        errors = self.validate(data)
        self.assertEqual(len(errors), 1)
        self.assertIn("only 2 usable floor cells", errors[0])
        self.assertEqual(errors.issues[0].level, 0)
        self.assertEqual(errors.issues[0].rect, (0, 0, 4, 2))

    def test_capacity_excludes_ramps_and_counts_duplicate_floors_once(self):
        data = spawn_map()
        data["levels"].append(empty_map()["levels"][0])
        data["levels"][0]["floors"].append(data["levels"][0]["floors"][0].copy())
        data["ramps"] = [{"lower_level": 0, "low": [1, 0], "high": [2, 2], "all": DEFAULT_ALIAS}]
        self.assertTrue(any("only 1 usable floor cells" in error for error in self.validate(data)))

    def test_movable_actor_count_is_not_limited_by_floor_count(self):
        data = spawn_map(100)
        data["actor_spawn_zones"][0]["kind"] = "scuttler"
        self.assertFalse(self.validate(data))

    def test_invalid_level_is_reported_without_reading_a_missing_floor_list(self):
        data = spawn_map()
        data["actor_spawn_zones"][0]["level"] = 5
        self.assertTrue(any("invalid level" in error for error in self.validate(data)))

    def test_nested_geometry_uses_the_parent_actor_catalog_for_capacity(self):
        data = empty_map(8, 8)
        child = spawn_map(3)
        child["levels"][0]["light_bridges"] = []
        data["nested_geometry"] = {"turret_room": child}
        errors = validate_document(
            data,
            MapCatalogs({}, {}, 0.1, {DEFAULT_ALIAS: True}),
            actor_kinds=["turret", "scuttler"],
            immovable_actor_kinds={"turret"},
        )
        issue = next(issue for issue in errors.issues if "usable floor cells" in issue.message)
        self.assertEqual(issue.map_name, "turret_room")


class SpawnCapacityWindowTests(WindowTestCase):
    def test_catalog_reload_updates_immovable_capacity_validation(self):
        data = spawn_map(3)
        data["levels"][0]["light_bridges"] = []
        self.assertTrue(any("usable floor cells" in error for error in self.window.validate(data)))
        with patch("map_editor.file_actions.load_immovable_actor_kinds", return_value=set()):
            self.window.reload_dependencies()
        self.assertFalse(any("usable floor cells" in error for error in self.window.validate(data)))

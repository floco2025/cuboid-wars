import json
import tempfile
import unittest
from pathlib import Path

from editor_fixtures import floor, nested
from map_editor.io import read_map, write_map
from map_editor.normalization import canonicalize_map, empty_map
from map_editor.validation import validate_map

KIND = "treasure"
BRIDGE_KIND = "skyway"


class FileIoTests(unittest.TestCase):
    def test_map_files_are_written_without_a_schema_version(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)

            wrapper = json.loads(path.read_text(encoding="utf-8"))
            self.assertNotIn("version", wrapper)
            self.assertNotIn("barrier_kinds", wrapper["map"])
            self.assertEqual(read_map(path), canonicalize_map(data))

    def test_plates_round_trip_through_the_file_format(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "switch": KIND},
            {"level": 0, "col": 1, "row": 0, "switch": "fireworks"},
        ]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"switch": "fireworks"}', text)
            self.assertEqual(read_map(path)["pressure_plates"], data["pressure_plates"])

    def test_bridges_switched_zones_and_nested_maps_round_trip_through_the_file_format(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["light_bridges"] = [{"col": 1, "row": 0, "kind": BRIDGE_KIND}]
        data["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "switch": BRIDGE_KIND}]
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "cols": [0, 1],
                "rows": [0, 1],
                "kind": "zapper",
                "count": 1,
                "respawn_secs": 90,
                "switch": BRIDGE_KIND,
            },
            {"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 2, "respawn_secs": 90},
        ]
        data["nested_maps"] = [
            {**nested("tile", 0, [0, 0], [1, 0]), "switch": BRIDGE_KIND},
            nested("tile", 0, [1, 1], [1, 1]),
        ]
        self.assertEqual(validate_map(data, [], [BRIDGE_KIND], switches=[BRIDGE_KIND]), [])

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"light_bridges": [', text)
            self.assertEqual(text.count('"switch": "skyway"'), 3)
            loaded = read_map(path)
            self.assertEqual(loaded["levels"][0]["light_bridges"], data["levels"][0]["light_bridges"])
            self.assertEqual(loaded["pressure_plates"], data["pressure_plates"])
            self.assertEqual(loaded["actor_spawn_zones"], data["actor_spawn_zones"])
            self.assertEqual(loaded["nested_maps"], data["nested_maps"])

    def test_nested_maps_round_trip_and_are_the_last_key(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [
            {**nested("cabin", 0, [2, 2], [4, 2]), "from_nudge": [0.3, 0.0, 0.0], "to_nudge": [0.0, -1.0, 1.01]}
        ]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "nested.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertGreater(text.index('"nested_maps"'), text.index('"ramps"'))
            self.assertEqual(read_map(path)["nested_maps"], data["nested_maps"])

import math
import unittest

from map_editor.core import call
from map_editor.geometry import normalized_wall, rects_overlap, zone_rect
from map_editor.normalization import edge_key
from map_editor.transforms import record_levels, record_rect

RECORDS = [
    ("floors", {"col": 3, "row": 4}),
    ("lights", {"col": 2, "row": 1, "side": "N"}),
    ("walls", {"c0": 4, "r0": 2, "c1": 3, "r1": 2}),
    ("barriers", {"c0": 1, "r0": 5, "c1": 1, "r1": 4}),
    ("ramps", {"lower_level": 1, "low": [6, 2], "high": [4, 3]}),
    ("ladders", {"lower_level": 0, "col": 2, "row": 2, "side": "E", "levels": 3}),
    ("ladders", {"lower_level": 2, "col": 2, "row": 2, "side": "E"}),
    ("nested_maps", {"level": 3, "from": [5, 1], "to": [2, 1], "to_level": 1}),
    ("nested_maps", {"level": 2, "from": [0, 0], "to": [0, 4]}),
    ("actor_spawn_zones", {"level": 1, "levels": 2, "cols": [1, 4], "rows": [2, 3]}),
    ("checkpoints", {"level": 0, "cols": [0, 2], "rows": [0, 2]}),
    ("items", {"level": 2, "col": 7, "row": 8}),
    # Authored mistakes stay in an open document until they are repaired.
    ("floors", {"col": "3", "row": None}),
    ("walls", {"c0": 1.9, "r0": True, "c1": math.inf}),
    ("ramps", {"lower_level": None, "low": [1], "high": "far"}),
    ("actor_spawn_zones", {"level": 1.5, "levels": 0, "cols": [2], "rows": None}),
    ("actor_spawn_zones", {"level": 1, "levels": 2.0, "cols": [1, 4], "rows": [2, 3]}),
    ("nested_maps", {"level": 1, "from": [0, 0], "to": [1, 1], "to_level": None}),
]


class CoreParityTests(unittest.TestCase):
    def test_python_coordinate_helpers_match_map_core(self):
        for name, entry in RECORDS:
            with self.subTest(name=name, entry=entry):
                self.assertEqual(record_rect(name, entry), tuple(call("record_rect", name, entry)))
                for level in (None, 4):
                    self.assertEqual(record_levels(entry, level), tuple(call("record_levels", entry, level)))
                if "cols" in entry:
                    self.assertEqual(zone_rect(entry), tuple(call("zone_rect", entry)))
                if "c0" in entry:
                    self.assertEqual(edge_key(entry), tuple(call("edge_key", entry)))

    def test_python_rectangle_helpers_match_map_core(self):
        for wall in ([3, 1, 2, 1], [2, 1, 3, 1], [2, 4, 2, 3]):
            self.assertEqual(normalized_wall(wall), call("normalized_wall", wall))
        rects = [(0, 0, 2, 2), (2, 0, 4, 2), (1, 1, 3, 3), (0, 2, 2, 4)]
        for a in rects:
            for b in rects:
                self.assertEqual(rects_overlap(a, b), call("rects_overlap", a, b))


if __name__ == "__main__":
    unittest.main()

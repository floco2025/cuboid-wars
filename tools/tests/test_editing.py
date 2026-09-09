import copy
import unittest

from map_editor.constants import FACES
from map_editor.editing import top_left_materials


class EditingTests(unittest.TestCase):
    def test_material_source_uses_spatial_order_not_record_or_wall_endpoint_order(self) -> None:
        pattern = dict(zip(FACES, ("a", "b", "c", "d", "e", "f")))
        cases = {
            "floors": ({"col": 2, "row": 1}, {"col": 1, "row": 2}),
            "walls": ({"c0": 3, "r0": 1, "c1": 2, "r1": 1}, {"c0": 0, "r0": 2, "c1": 1, "r1": 2}),
            "ramps": ({"low": [2, 1], "high": [5, 2]}, {"low": [0, 3], "high": [3, 4]}),
        }
        for name, (first, second) in cases.items():
            with self.subTest(name=name):
                entries = [{**second, **dict.fromkeys(FACES, "other")}, {**first, **pattern}]
                before = copy.deepcopy(entries)
                self.assertEqual(top_left_materials(entries, name), pattern)
                self.assertEqual(entries, before)

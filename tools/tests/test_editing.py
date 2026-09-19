import copy
import unittest

from map_editor.constants import FACES, TERRAIN_FACES
from editor_fixtures import floor
from map_editor.editing import paint_terrain, top_left_materials
from map_editor.normalization import empty_map


class EditingTests(unittest.TestCase):
    def test_terrain_paint_creates_slabs_and_replaces_other_floor_kinds(self) -> None:
        data = empty_map(4, 4)
        data["levels"][0]["floors"] = [floor(0, 0), floor(2, 2)]
        data["levels"][0]["inaccessible_floors"] = [floor(1, 0)]

        result = paint_terrain(data, 0, (0, 0, 2, 2), "stone")

        expected = [
            {"col": col, "row": row, **dict.fromkeys(TERRAIN_FACES, "stone")} for row in range(2) for col in range(2)
        ]
        self.assertEqual(result["levels"][0]["terrain"], expected)
        self.assertEqual(result["levels"][0]["floors"], [floor(2, 2)])
        self.assertEqual(result["levels"][0]["inaccessible_floors"], [])
        self.assertEqual(data["levels"][0]["terrain"], [])

    def test_material_source_uses_spatial_order_not_record_or_wall_endpoint_order(self) -> None:
        pattern = dict(zip(FACES, ("a", "b", "c", "d", "e", "f")))
        cases = {
            "floors": ({"col": 2, "row": 1}, {"col": 1, "row": 2}),
            "walls": ({"c0": 3, "r0": 1, "c1": 2, "r1": 1}, {"c0": 0, "r0": 2, "c1": 1, "r1": 2}),
            "ramps": (
                {"cols": [2, 5], "rows": [1, 2], "direction": "E"},
                {"cols": [0, 3], "rows": [3, 4], "direction": "E"},
            ),
        }
        for name, (first, second) in cases.items():
            with self.subTest(name=name):
                entries = [{**second, **dict.fromkeys(FACES, "other")}, {**first, **pattern}]
                before = copy.deepcopy(entries)
                self.assertEqual(top_left_materials(entries, name), pattern)
                self.assertEqual(entries, before)

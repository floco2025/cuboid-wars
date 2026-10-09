import unittest

from editor_fixtures import floor
from map_editor.constants import TERRAIN_FACES
from map_editor.editing import paint_terrain
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

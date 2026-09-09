import unittest

from map_editor.geometry import ramp_axis, ramp_cells, wall_segments_between


class GeometryTests(unittest.TestCase):
    def test_wall_segments_are_unit_length_and_canonical(self) -> None:
        self.assertEqual(
            wall_segments_between((3, 2), (0, 2)),
            [[2, 2, 3, 2], [1, 2, 2, 2], [0, 2, 1, 2]],
        )

    def test_ramp_cells_and_axis_follow_its_footprint(self) -> None:
        ramp = {"low": [3, 1], "high": [0, 3]}
        self.assertEqual(ramp_axis(ramp), "west")
        self.assertEqual(ramp_cells(ramp), {(0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)})

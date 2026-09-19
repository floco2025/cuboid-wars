import unittest

from map_editor.geometry import drag_direction, ramp_cells, ramp_ghosts_on, wall_segments_between


class GeometryTests(unittest.TestCase):
    def test_wall_segments_are_unit_length_and_canonical(self) -> None:
        self.assertEqual(
            wall_segments_between((3, 2), (0, 2)),
            [[2, 2, 3, 2], [1, 2, 2, 2], [0, 2, 1, 2]],
        )

    def test_ramp_cells_cover_its_footprint(self) -> None:
        ramp = {"cols": [0, 3], "rows": [1, 3], "direction": "W"}
        self.assertEqual(ramp_cells(ramp), {(0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)})

    def test_a_drag_rises_along_its_dominant_axis_and_a_still_pointer_has_no_direction(self) -> None:
        for motion, direction in (
            ((0.3, 0.1), "E"),
            ((-0.3, 0.2), "W"),
            ((0.1, 0.4), "S"),
            ((0.2, -0.4), "N"),
            ((0.0, 0.0), None),
        ):
            with self.subTest(motion=motion):
                self.assertEqual(drag_direction(*motion), direction)

    def test_a_ramp_ghosts_on_every_other_level_it_touches_and_the_one_below(self) -> None:
        ramp = {"lower_level": 2, "levels": 2}
        self.assertEqual([level for level in range(7) if ramp_ghosts_on(ramp, level)], [1, 3, 4])

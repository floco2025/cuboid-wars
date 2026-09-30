import unittest

from editor_fixtures import floor
from map_editor.floor_footprints import FloorFootprints, ramp_landing_edges
from map_editor.normalization import empty_map


class FloorFootprintTests(unittest.TestCase):
    def data(self, cells=()):
        data = empty_map(8, 8)
        data["levels"][0]["floors"] = [floor(c, r) for c, r in cells]
        return data

    def test_isolated_and_map_border_tiles_extend_on_all_sides(self):
        footprints = FloorFootprints(self.data([(0, 0)]), 4, 0.4)
        self.assertEqual(footprints.rectangles(0, 0, 0), [(-0.2, -0.2, 4.2, 4.2)])

    def test_shared_edges_stop_at_grid_line_including_blocked_floors(self):
        data = self.data([(2, 2)])
        data["levels"][0]["inaccessible_floors"] = [floor(1, 2), floor(3, 2), floor(2, 1), floor(2, 3)]
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(footprints.rectangles(0, 2, 2), [(8, 8, 12, 12)])

    def test_diagonal_neighbors_trim_corner_fillers_on_each_side(self):
        cases = [
            ((1, 1), [(7.8, 8, 12.2, 12.2), (8.2, 7.8, 12.2, 8)]),
            ((3, 1), [(7.8, 8, 12.2, 12.2), (7.8, 7.8, 11.8, 8)]),
            ((1, 3), [(7.8, 7.8, 12.2, 12), (8.2, 12, 12.2, 12.2)]),
            ((3, 3), [(7.8, 7.8, 12.2, 12), (7.8, 12, 11.8, 12.2)]),
        ]
        for neighbor, expected in cases:
            with self.subTest(neighbor=neighbor):
                footprints = FloorFootprints(self.data([(2, 2), neighbor]), 4, 0.4)
                self.assertEqual(footprints.rectangles(0, 2, 2), expected)

    def test_all_diagonal_neighbors_leave_two_narrow_fillers(self):
        footprints = FloorFootprints(self.data([(2, 2), (1, 1), (3, 1), (1, 3), (3, 3)]), 4, 0.4)
        self.assertEqual(
            footprints.rectangles(0, 2, 2), [(7.8, 8, 12.2, 12), (8.2, 7.8, 11.8, 8), (8.2, 12, 11.8, 12.2)]
        )

    def test_an_empty_cell_gets_the_slab_a_floor_there_would_have(self):
        data = self.data([(2, 2)])
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(footprints.rectangles(0, 3, 2), [(12, 7.8, 16.2, 12.2)])
        self.assertEqual(len(data["levels"][0]["floors"]), 1)

    def test_edge_points_lie_on_the_slabs_extensions_and_fillers(self):
        footprints = FloorFootprints(self.data([(2, 2), (3, 2), (3, 1)]), 4, 0.4)
        self.assertEqual(footprints.edge_point(0, 2, 2, "W", 0.5), (7.8, 10.0))
        self.assertEqual(footprints.edge_point(0, 2, 2, "E", 0.5), (12, 10.0))
        self.assertEqual(footprints.edge_point(0, 2, 2, "S", 0.25), (9.0, 12.2))
        # The corner filler toward the diagonal slab stops a pad short of it.
        self.assertEqual(footprints.rectangles(0, 2, 2), [(7.8, 8, 12, 12.2), (7.8, 7.8, 11.8, 8)])
        self.assertEqual(footprints.edge_point(0, 2, 2, "N", 0.5), (10.0, 7.8))
        self.assertEqual(footprints.edge_point(0, 2, 2, "N", 0.99), (11.96, 8))

    def test_support_reaches_through_a_neighbours_extension(self):
        footprints = FloorFootprints(self.data([(2, 2)]), 4, 0.4)
        self.assertEqual(footprints.supporting_cell(0, 10.0, 10.0), (2, 2))
        self.assertEqual(footprints.supporting_cell(0, 12.1, 10.0), (2, 2))
        self.assertEqual(footprints.supporting_cell(0, 7.85, 7.85), (2, 2))
        self.assertIsNone(footprints.supporting_cell(0, 12.3, 10.0))
        self.assertEqual(footprints.supporting_cell(0, 12.3, 10.0, reach=0.2), (2, 2))
        self.assertIsNone(footprints.supporting_cell(0, 12.5, 10.0, reach=0.2))
        self.assertIsNone(footprints.supporting_cell(1, 10.0, 10.0))
        self.assertTrue(footprints.floor_under(0, 12.2, 12.2))
        self.assertFalse(footprints.floor_under(0, -3.0, 10.0))

    def test_ramp_ends_skip_only_the_fillers_along_their_own_edges(self):
        data = self.data([(1, 1), (3, 1), (2, 2)])
        data["levels"].append(dict(data["levels"][0]))
        data["ramps"] = [{"lower_level": 0, "cols": [2, 3], "rows": [0, 2], "direction": "S"}]
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(ramp_landing_edges(data), [{("h", 0, 2)}, {("h", 2, 2)}])
        self.assertEqual(len(footprints.rectangles(0, 2, 2)), 2)
        self.assertEqual(footprints.rectangles(1, 2, 2), [(7.8, 8, 12.2, 12.2)])
        data["ramps"] = [{"lower_level": 0, "cols": [2, 3], "rows": [3, 5], "direction": "N"}]
        self.assertEqual(ramp_landing_edges(data), [{("h", 5, 2)}, {("h", 3, 2)}])
        data["ramps"] = [{"lower_level": 0, "cols": [0, 2], "rows": [2, 3], "direction": "E"}]
        self.assertEqual(ramp_landing_edges(data), [{("v", 2, 0)}, {("v", 2, 2)}])
        # A one-cell ramp rising two storeys lands along the side it rises toward, two levels up.
        data["levels"].append(dict(data["levels"][0]))
        data["ramps"] = [{"lower_level": 0, "levels": 2, "cols": [1, 2], "rows": [2, 3], "direction": "W"}]
        self.assertEqual(ramp_landing_edges(data), [{("v", 2, 2)}, set(), {("v", 2, 1)}])

    def test_ramp_landings_meet_slopes_without_slab_overhangs(self):
        cases = [
            ([3, 4], [1, 3], "S", [(11.8, 12, 16.2, 16.2)]),
            ([3, 4], [4, 6], "N", [(11.8, 11.8, 16.2, 16)]),
            ([1, 3], [3, 4], "E", [(12, 11.8, 16.2, 16.2)]),
            ([4, 6], [3, 4], "W", [(11.8, 11.8, 16, 16.2)]),
        ]
        for cols, rows, direction, expected in cases:
            with self.subTest(direction=direction):
                data = self.data([(3, 3)])
                data["levels"].append(dict(data["levels"][0]))
                data["ramps"] = [{"lower_level": 0, "cols": cols, "rows": rows, "direction": direction}]
                footprints = FloorFootprints(data, 4, 0.4)
                self.assertEqual(footprints.rectangles(1, 3, 3), expected)
                self.assertEqual(footprints.rectangles(0, 3, 3), [(11.8, 11.8, 16.2, 16.2)])


if __name__ == "__main__":
    unittest.main()

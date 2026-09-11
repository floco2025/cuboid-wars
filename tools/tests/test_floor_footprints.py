from math import hypot
import unittest

from editor_fixtures import WindowTestCase, floor
from map_editor.floor_footprints import FloorFootprints, corner_filler_skips
from map_editor.jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED, FallSettings, JumpSettings, calculate_reach
from map_editor.normalization import empty_level, empty_map


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

    def test_gap_uses_rectangles_without_filling_missing_corners(self):
        footprints = FloorFootprints(self.data([(2, 2), (3, 1)]), 4, 0.4)
        self.assertAlmostEqual(footprints.distance((0, 2, 2), (0, 4, 0)), hypot(3.6, 4))

    def test_hypothetical_floor_uses_its_neighbors_without_modifying_map(self):
        data = self.data([(2, 2)])
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(footprints.rectangles(0, 3, 2), [(12, 7.8, 16.2, 12.2)])
        self.assertAlmostEqual(footprints.distance((0, 2, 2), (0, 5, 2)), 7.6)
        self.assertEqual(len(data["levels"][0]["floors"]), 1)

    def test_hypothetical_origin_and_target_see_each_other_only_on_same_level(self):
        data = self.data()
        data["levels"].append(empty_level(1))
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(footprints.distance((0, 2, 2), (0, 3, 1)), 0)
        self.assertAlmostEqual(footprints.distance((0, 2, 2), (1, 5, 2)), 7.6)
        self.assertTrue(all(not cells for cells in footprints.cells))

    def test_ramp_high_ends_skip_only_upper_level_north_south_fillers(self):
        data = self.data([(1, 1), (3, 1), (2, 2)])
        data["levels"].append(dict(data["levels"][0]))
        data["ramps"] = [{"lower_level": 0, "low": [2, 0], "high": [3, 2]}]
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertEqual(corner_filler_skips(data), [set(), {(2, 2)}])
        self.assertEqual(len(footprints.rectangles(0, 2, 2)), 2)
        self.assertEqual(footprints.rectangles(1, 2, 2), [(7.8, 8, 12.2, 12.2)])
        data["ramps"] = [{"lower_level": 0, "low": [3, 5], "high": [2, 3]}]
        self.assertEqual(corner_filler_skips(data), [set(), {(3, 2)}])
        data["ramps"] = [{"lower_level": 0, "low": [0, 2], "high": [2, 3]}]
        self.assertEqual(corner_filler_skips(data), [set(), set()])

    def test_screenshot_jump_accounts_for_actual_tile_edges(self):
        data = empty_map(16, 10)
        data["levels"][0]["floors"] = [
            floor(c, r) for c, r in ((5, 4), (6, 4), (5, 5), (6, 5), (5, 6), (6, 6), (9, 7), (10, 7))
        ]
        footprints = FloorFootprints(data, 4, 0.4)
        self.assertAlmostEqual(footprints.distance((0, 6, 5), (0, 9, 7)), 8.4970583145)
        settings = JumpSettings(4, 2.4, 12, 5, 5, 1.818, 24, 13.2, 0.4, FallSettings(8, 15, 100))
        reach = calculate_reach(settings, (0, 6, 5), data, running=True, margin=0.1)
        self.assertEqual(sum(reach[0, 9, 7]), ANTI_GRAVITY | BOTH)
        reach = calculate_reach(settings, (0, 6, 5), data, running=True, margin=0.05)
        self.assertEqual(sum(reach[0, 9, 7]), SPEED | ANTI_GRAVITY | BOTH)

    def test_range_search_includes_expansion_past_the_grid_distance_bound(self):
        settings = JumpSettings(1, 1, 1, 1, 1, 1, 2, 2, 0.4, FallSettings(8, 15, 100))
        reach = calculate_reach(settings, (0, 0, 0), self.data(), running=True, margin=0.2)
        self.assertIn(NORMAL, reach[0, 2, 0])


class FloorFootprintWindowTests(WindowTestCase):
    def test_floor_changes_recalculate_without_losing_origin_and_undo_restores_results(self):
        window = self.window
        overlay = window.jump_reach
        overlay.settings = JumpSettings(4, 2.4, 12, 5, 5, 1.818, 24, 13.2, 0.4, FallSettings(8, 15, 100))
        window.doc.replace_with_new(empty_map(8, 8))
        overlay.select(2, 2)
        overlay.margin.setValue(0.07)
        initial = dict(overlay.results)
        self.assertIn(SPEED, initial[0, 5, 4])
        window.add_floor_rect((2, 3), (2, 3))
        self.assertEqual(overlay.origin, (0, 2, 2))
        self.assertNotIn(SPEED, overlay.results.get((0, 5, 4), {}))
        window.undo_stack.undo()
        self.assertEqual(overlay.results, initial)
        window.undo_stack.redo()
        self.assertNotIn(SPEED, overlay.results.get((0, 5, 4), {}))

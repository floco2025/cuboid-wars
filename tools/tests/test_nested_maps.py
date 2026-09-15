import unittest
from unittest.mock import Mock

from editor_fixtures import EditorHost, NESTED_SHAPES, nested
from map_editor.nesting import NestedMapShape, NestedMotion, nested_map_cycle, nested_map_label, nested_map_rest_points
from map_editor.normalization import empty_map


class NestedMapTests(unittest.TestCase):
    def test_a_click_places_a_still_nested_map_and_a_drag_a_sliding_one(self) -> None:
        host = EditorHost(empty_map(8, 8), [])
        host.place_nested_map((1, 1), (1, 1), NestedMotion("cabin", 0, 2.0, 1.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)))
        host.place_nested_map((4, 1), (6, 3), NestedMotion("cabin", 0, 3.0, 0.5, 2.0, (0.4, 0.0, 0.0), (0.0, 0.0, 0.0)))

        self.assertEqual(
            host.map_data["nested_maps"],
            [
                nested("cabin", 0, [1, 1], [1, 1]),
                {
                    **nested("cabin", 0, [4, 1], [6, 3]),
                    "travel_secs": 3.0,
                    "pause_secs": 0.5,
                    "phase_secs": 2.0,
                    "from_nudge": [0.4, 0.0, 0.0],
                },
            ],
        )
        host.place_nested_map((2, 2), (2, 2), NestedMotion("", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)))
        self.assertEqual(len(host.map_data["nested_maps"]), 2)
        self.assertTrue(host.statuses[-1].startswith("Nested map not placed"))

    def test_placement_refuses_a_cell_where_a_nested_map_starts_or_ends(self) -> None:
        data = empty_map(8, 8)
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [5, 1])]
        host = EditorHost(data, [])

        host.add_nested_map((5, 1), (5, 4))
        self.assertTrue(host.statuses[-1].startswith("A nested map already ends here"))
        host.add_nested_map((1, 1), (3, 3))
        self.assertTrue(host.statuses[-1].startswith("A nested map already starts here"))
        self.assertEqual(host.map_data["nested_maps"], [nested("cabin", 0, [1, 1], [5, 1])])

    def test_a_nested_maps_switch_and_response_are_written_only_while_set(self) -> None:
        host = EditorHost(empty_map(8, 8), [])
        motion = NestedMotion("cabin", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), "lift", True)
        host.place_nested_map((1, 1), (4, 1), motion)
        entry = host.map_data["nested_maps"][0]
        self.assertEqual((entry["switch"], entry["switch_inverted"]), ("lift", True))
        self.assertEqual(NestedMotion.from_entry({**entry, "to_level": 0, "phase_secs": 0.0}).switch, "lift")
        host.place_nested_map((2, 2), (2, 2), NestedMotion("cabin", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)))
        entry = host.map_data["nested_maps"][1]
        self.assertNotIn("switch", entry)
        self.assertNotIn("switch_inverted", entry)
        self.assertFalse(NestedMotion.from_entry(entry).switch_inverted)

    def test_placing_on_the_same_start_cell_replaces_the_old_nested_map(self) -> None:
        host = EditorHost(empty_map(8, 8), [])
        host.place_nested_map((1, 1), (4, 1), NestedMotion("cabin", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)))
        host.place_nested_map(
            (1, 1), (1, 4), NestedMotion("loop_a", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
        )

        self.assertEqual([(e["map"], e["to"]) for e in host.map_data["nested_maps"]], [("loop_a", [1, 4])])

    def test_nested_cycle_check_visits_a_shared_dependency_once(self) -> None:
        graph = {
            "left": NestedMapShape(1, 1, 1, ("shared",)),
            "right": NestedMapShape(1, 1, 1, ("shared",)),
            "shared": NestedMapShape(1, 1, 1, ()),
        }
        lookup = Mock(side_effect=graph.get)
        self.assertIsNone(nested_map_cycle("root", [{"map": "left"}, {"map": "right"}], lookup))
        self.assertEqual([call.args[0] for call in lookup.call_args_list].count("shared"), 1)

    def test_nested_map_cycle_names_the_loop(self) -> None:
        self.assertEqual(
            nested_map_cycle("home", [nested("loop_a", 0, [0, 0], [0, 0])], NESTED_SHAPES.get),
            ["loop_a", "loop_b", "loop_a"],
        )
        self.assertIsNone(nested_map_cycle("home", [nested("cabin", 0, [0, 0], [0, 0])], NESTED_SHAPES.get))

    def test_nudges_shift_each_footprint_by_wall_widths_in_the_canvas_plane(self) -> None:
        entry = {**nested("cabin", 0, [2, 3], [6, 3]), "from_nudge": [1.0, -2.0, 0.0], "to_nudge": [-1.01, 0.0, 3.0]}
        start, end = nested_map_rest_points(entry, 0.1)
        self.assertAlmostEqual(start[0], 2.1)
        self.assertAlmostEqual(start[1], 3.0)
        self.assertAlmostEqual(end[0], 5.899)
        self.assertAlmostEqual(end[1], 3.3)
        self.assertEqual(nested_map_label("cabin", entry["from_nudge"]), "cabin y-2")
        self.assertEqual(nested_map_label("cabin", entry["to_nudge"]), "cabin")

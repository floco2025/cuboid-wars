import unittest
from unittest.mock import Mock, patch

from PySide6.QtCore import QPointF

from editor_fixtures import EditorHost, NESTED_SHAPES, WindowTestCase, floor, furnished_map, nested
from map_editor.dialogs import MotionDialog
from map_editor.io import read_map
from map_editor.nesting import NestedMapShape, NestedMotion, nested_map_cycle, nested_map_label, nested_map_rest_points
from map_editor.normalization import empty_map, normalize_map
from map_editor.validation import validate_map


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

    def test_invalid_motion_is_preserved_for_validation(self):
        data = empty_map(8, 8)
        data["nested_maps"] = [{**nested("cabin", 0, [1, 1], [3, 1]), "motion": "yes"}]
        normalized = normalize_map(data)
        self.assertEqual(normalized["nested_maps"][0]["motion"], "yes")
        self.assertTrue(
            any("motion must be cycle or follow_switch" in error for error in validate_map(normalized, [], []))
        )

    def test_follow_motion_requires_a_switch(self):
        data = empty_map(8, 8)
        data["nested_maps"] = [{**nested("cabin", 0, [1, 1], [3, 1]), "motion": "follow_switch"}]
        self.assertTrue(any("Follow switch motion requires a switch" in error for error in validate_map(data, [], [])))

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


class NestedMotionWindowTests(WindowTestCase):
    def test_follow_motion_preserves_cycle_settings_through_placement_sampling_undo_and_save(self):
        window = self.window
        data = furnished_map()
        data["switches"] = [{"id": "barrier_1", "activation": "momentary", "reset_on_player_death": "never"}]
        child = empty_map(1, 1)
        child["levels"][0]["floors"] = [floor(0, 0)]
        data["nested_geometry"] = {"cabin": child}
        window.doc.replace_with_new(data)
        dialog = MotionDialog(window, 1, 0, None, "Nested Map Defaults", ["cabin"], ["barrier_1"])
        self.assertTrue(dialog._pause.isEnabled())
        self.assertTrue(dialog._phase.isEnabled())
        dialog._pause.setValue(1.5)
        dialog._phase.setValue(2.5)
        dialog._motion.setCurrentIndex(dialog._motion.findData("follow_switch"))
        self.assertFalse(dialog._pause.isEnabled())
        self.assertFalse(dialog._phase.isEnabled())
        self.assertEqual((dialog._pause.value(), dialog._phase.value()), (1.5, 2.5))
        with patch("map_editor.dialogs.motion.QMessageBox.warning") as warning:
            dialog.accept()
        warning.assert_called_once()
        self.assertEqual(dialog.result(), 0)
        dialog._switch.setCurrentIndex(dialog._switch.findData("barrier_1"))
        self.assertTrue(dialog.control.response.isEnabled())
        motion = NestedMotion(dialog._map.currentText(), *dialog.motion())
        window.place_nested_map((3, 3), (5, 3), motion)
        entry = window.map_data["nested_maps"][0]
        self.assertEqual(entry["motion"], "follow_switch")
        self.assertEqual(NestedMotion.from_entry(entry), motion)
        window.sample_at(QPointF(3.5, 3.5))
        self.assertEqual(window.recent_nested_map, motion)
        window.undo_stack.undo()
        self.assertEqual(window.map_data["nested_maps"], [])
        window.undo_stack.redo()
        window.doc.write(self.path)
        self.assertEqual(read_map(self.path)["nested_maps"][0]["motion"], "follow_switch")
        restored = MotionDialog(window, 1, 0, motion, "Nested Map Defaults", ["cabin"], ["barrier_1"])
        self.assertEqual(restored._motion.currentData(), "follow_switch")
        self.assertFalse(restored._pause.isEnabled())
        restored._motion.setCurrentIndex(restored._motion.findData("cycle"))
        self.assertTrue(restored._pause.isEnabled())
        self.assertTrue(restored._phase.isEnabled())
        self.assertEqual((restored._pause.value(), restored._phase.value()), (1.5, 2.5))
        restored._switch.setCurrentIndex(restored._switch.findData(""))
        self.assertFalse(restored.control.response.isEnabled())
        self.assertNotIn("switch", NestedMotion("cabin", *restored.motion()).to_entry())
        dialog.close()
        restored.close()

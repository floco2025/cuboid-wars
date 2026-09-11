import copy
import unittest

from editor_fixtures import DEFAULT_ALIAS, faces, floor, nested
from map_editor.editing import paint_floors
from map_editor.erasing import erase_cell_rect
from map_editor.normalization import empty_level, empty_map, normalize_map
from map_editor.transforms import insert_level_data, record_lists, remove_level_data, resize_map_data, translate_map

KIND = "treasure"
BRIDGE_KIND = "skyway"


class ResizeTests(unittest.TestCase):
    def test_center_resize_translates_every_coordinate_family(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(empty_level(1))
        level = data["levels"][0]
        level["floors"] = [floor(1, 1)]
        level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, **faces()}]
        level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        level["light_bridges"] = [{"col": 1, "row": 3, "kind": BRIDGE_KIND}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 3], "rows": [1, 3], "kind": "scuttler", "count": 1, "respawn_secs": 90}
        ]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "gold"}]
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "type": "barrier", "kind": KIND}]
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        data["ladders"] = [{"lower_level": 0, "col": 1, "row": 1, "side": "N", "levels": 1}]

        result = resize_map_data(data, 6, 6, 1, 1)

        self.assertEqual((result["levels"][0]["floors"][0]["col"], result["levels"][0]["floors"][0]["row"]), (2, 2))
        wall = result["levels"][0]["walls"][0]
        self.assertEqual((wall["c0"], wall["r0"], wall["c1"], wall["r1"]), (2, 2, 3, 2))
        self.assertEqual(result["actor_spawn_zones"][0]["cols"], [2, 4])
        bridge = result["levels"][0]["light_bridges"][0]
        self.assertEqual((bridge["col"], bridge["row"], bridge["kind"]), (2, 4, BRIDGE_KIND))
        self.assertEqual((result["items"][0]["col"], result["items"][0]["row"]), (2, 2))
        self.assertEqual((result["pressure_plates"][0]["col"], result["pressure_plates"][0]["row"]), (2, 2))
        self.assertEqual(result["ramps"][0]["low"], [2, 2])
        self.assertEqual((result["ladders"][0]["col"], result["ladders"][0]["row"]), (2, 2))

    def test_resize_drops_a_nested_map_with_an_anchor_outside(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [nested("cabin", 0, [0, 0], [5, 0]), nested("cabin", 0, [2, 2], [2, 2])]
        resized = resize_map_data(data, 5, 6, 0, 0)
        self.assertEqual([e["from"] for e in resized["nested_maps"]], [[2, 2]])


class LevelTests(unittest.TestCase):
    def test_remove_level_drops_spanning_nested_maps_and_renumbers_the_rest(self) -> None:
        data = empty_map(6, 6)
        data["levels"].append({**empty_level(1), "floors": [floor(0, 0)]})
        data["levels"].append({**empty_level(2), "floors": [floor(0, 0)]})
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [1, 1], 1), nested("cabin", 2, [3, 3], [3, 3])]
        after = remove_level_data(data, 1)
        self.assertEqual([(e["level"], e["to_level"]) for e in after["nested_maps"]], [(1, 1)])

    def test_insert_level_keeps_each_nested_end_on_its_storey(self) -> None:
        data = empty_map(6, 6)
        data["levels"].append({**empty_level(1), "floors": [floor(0, 0)]})
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [1, 1], 1)]
        after = insert_level_data(data, 1)
        self.assertEqual((after["nested_maps"][0]["level"], after["nested_maps"][0]["to_level"]), (0, 2))

    def test_transform_and_erase_helpers_leave_their_input_unchanged(self) -> None:
        data = normalize_map(empty_map())
        data = paint_floors(data, 0, (2, 2, 3, 3), DEFAULT_ALIAS)
        before = copy.deepcopy(data)
        moved = translate_map(data, 2, 3)
        erased = erase_cell_rect(data, 0, (2, 2), (2, 2), False)
        self.assertEqual(data, before)
        self.assertEqual(moved["levels"][0]["floors"][0]["col"], 4)
        self.assertEqual(erased["levels"][0]["floors"], [])
        partial = {"levels": [{}]}
        list(record_lists(partial))
        self.assertEqual(partial, {"levels": [{}]})
        with self.assertRaises(ValueError):
            remove_level_data(data, 0)

import copy
import unittest

from editor_fixtures import DEFAULT_ALIAS, faces, floor, nested
from map_editor.editing import paint_floors
from map_editor.erasing import erase_cell_rect
from map_editor.normalization import empty_level, empty_map, normalize_map
from map_editor.transforms import (
    edit_levels_data,
    insert_level_data,
    map_content_bounds,
    record_lists,
    remove_level_data,
    resize_map_data,
    resize_map_offset,
    translate_map,
)

KIND = "treasure"
BRIDGE_KIND = "skyway"


class ResizeTests(unittest.TestCase):
    def test_content_bounds_cover_all_levels_and_spanning_objects(self):
        data = empty_map(20, 20)
        data["player_spawn_zones"] = []
        data["levels"] = [empty_level(i) for i in range(9)]
        data["levels"][2]["floors"] = [floor(4, 6)]
        data["levels"][2]["walls"] = [{"c0": 3, "r0": 6, "c1": 4, "r1": 6, **faces()}]
        data["levels"][4]["terrain"] = [{"col": 5, "row": 5}]
        data["actor_spawn_zones"] = [{"level": 3, "cols": [4, 8], "rows": [6, 10], "levels": 2}]
        data["items"] = [{"level": 5, "col": 10, "row": 7, "type": "gold"}]
        data["ramps"] = [{"lower_level": 1, "low": [4, 6], "high": [5, 9]}]
        data["ladders"] = [{"lower_level": 3, "col": 4, "row": 7, "side": "N", "levels": 3}]
        before = copy.deepcopy(data)
        bounds = map_content_bounds(data)
        self.assertEqual(bounds.rect, (3, 5, 11, 10))
        self.assertEqual((bounds.first_level, bounds.last_level), (1, 6))
        after = resize_map_offset(data, 8, 5, -3, -5)
        self.assertEqual(len(after["levels"]), 9)
        self.assertEqual(after["items"][0], {"level": 5, "col": 7, "row": 2, "type": "gold"})
        self.assertEqual(after["actor_spawn_zones"][0]["rows"], [1, 5])
        self.assertEqual(after["ramps"][0]["low"], [1, 1])
        self.assertEqual(data, before)

    def test_content_bounds_include_nested_footprints_nudges_and_both_motion_ends(self):
        data = empty_map(20, 20)
        data["player_spawn_zones"] = []
        data["levels"] = [empty_level(i) for i in range(10)]
        child = empty_map(3, 2)
        child["levels"].append(empty_level(1))
        data["nested_geometry"] = {"cabin": child}
        data["nested_maps"] = [nested("cabin", 2, [4, 5], [11, 8], 5)]
        data["nested_maps"][0]["from_nudge"] = [-1, -1, -1]
        data["nested_maps"][0]["to_nudge"] = [1, 1, 1]
        bounds = map_content_bounds(data, wall_width_cells=0.25, wall_height_levels=0.25)
        self.assertEqual(bounds.rect, (3, 4, 15, 11))
        self.assertEqual((bounds.first_level, bounds.last_level), (1, 7))

    def test_empty_and_boundary_only_maps_keep_at_least_one_cell_and_level(self):
        data = empty_map(9, 8)
        data["player_spawn_zones"] = []
        data["levels"].append(empty_level(1))
        bounds = map_content_bounds(data)
        self.assertEqual(bounds.rect, (0, 0, 1, 1))
        self.assertEqual((bounds.first_level, bounds.last_level), (0, 0))
        data["levels"][1]["walls"] = [{"c0": 8, "r0": 8, "c1": 9, "r1": 8, **faces()}]
        bounds = map_content_bounds(data)
        self.assertEqual(bounds.rect, (8, 7, 9, 8))
        self.assertEqual((bounds.first_level, bounds.last_level), (1, 1))
        after = resize_map_offset(data, 1, 1, -8, -7)
        wall = after["levels"][1]["walls"][0]
        self.assertEqual((wall["c0"], wall["r0"], wall["c1"], wall["r1"]), (0, 1, 1, 1))

    def test_fitting_does_not_enlarge_for_nested_geometry_already_outside_the_grid(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["nested_geometry"] = {"cabin": empty_map(6, 7)}
        data["nested_maps"] = [nested("cabin", 0, [5, 5], [5, 5])]
        self.assertEqual(map_content_bounds(data).rect, (5, 5, 8, 8))

    def test_center_resize_translates_every_coordinate_family(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(empty_level(1))
        level = data["levels"][0]
        level["floors"] = [floor(1, 1)]
        level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, **faces()}]
        level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        level["light_bridges"] = [{"col": 1, "row": 3, "kind": BRIDGE_KIND}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 3], "rows": [1, 3], "kind": "scuttler", "count": [1], "respawn_secs": 90}
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
    def test_batch_level_changes_remap_spans_and_keep_original_geometry(self):
        data = empty_map(6, 6)
        data["levels"] += [empty_level(1), empty_level(2)]
        data["levels"][2]["floors"] = [floor(1, 1)]
        data["actor_spawn_zones"] = [{"level": 0, "levels": 3, "cols": [0, 2], "rows": [0, 2], "kind": "turret"}]
        data["ladders"] = [{"lower_level": 0, "levels": 2, "col": 3, "row": 3, "side": "N"}]
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [3, 1], 2)]
        before = copy.deepcopy(data)
        after = edit_levels_data(data, [(0, "Ground"), (None, "Landing"), (1, "Middle"), (2, "Roof")])
        self.assertEqual(after["ladders"][0]["levels"], 3)
        self.assertEqual(after["actor_spawn_zones"][0]["levels"], 4)
        self.assertEqual(after["nested_maps"][0]["to_level"], 3)
        self.assertEqual(after["levels"][3]["floors"], data["levels"][2]["floors"])
        self.assertEqual(data, before)

    def test_replacing_all_levels_removes_geometry_but_keeps_map_catalogs(self):
        data = empty_map(6, 6)
        data["levels"] += [empty_level(1)]
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["actor_spawn_zones"] = [{"level": 0, "levels": 2, "cols": [0, 2], "rows": [0, 2], "kind": "turret"}]
        data["switches"] = [{"id": "lift", "activation": "momentary", "reset_on_player_death": "never"}]
        after = edit_levels_data(data, [(None, "New")])
        self.assertEqual(after["levels"], [{**empty_level(0), "name": "New"}])
        self.assertEqual(after["actor_spawn_zones"], [])
        self.assertEqual(after["switches"], data["switches"])

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

import copy
import unittest

from editor_fixtures import DEFAULT_ALIAS, floor, nested
from map_editor.block_transforms import transform_block
from map_editor.geometry import wall_endpoints_for_cell_side
from map_editor.normalization import empty_level, empty_map, normalize_map


class BlockTransformTests(unittest.TestCase):
    def block(self):
        data = empty_map(5, 4)
        data["player_spawn_zones"] = []
        data["levels"].append(empty_level(1))
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["levels"][0]["walls"] = [
            {
                "c0": 1,
                "r0": 1,
                "c1": 2,
                "r1": 1,
                "north": "north-material",
                "east": "east-material",
                "south": "south-material",
                "west": "west-material",
                "top": "top-material",
                "bottom": "bottom-material",
            }
        ]
        data["levels"][0]["lights"] = [{"col": 1, "row": 1, "side": "N", "kind": "utility"}]
        data["ramps"] = [{"lower_level": 0, "low": [1, 2], "high": [4, 3], "all": DEFAULT_ALIAS}]
        data["ladders"] = [{"col": 4, "row": 3, "side": "W", "lower_level": 0, "levels": 1}]
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "levels": 2,
                "cols": [0, 2],
                "rows": [0, 1],
                "kind": "scuttler",
                "count": [2, 4],
                "respawn_secs": None,
            }
        ]
        data["items"] = [{"col": 1, "row": 1, "level": 0, "type": "gold"}]
        data["pressure_plates"] = [{"col": 1, "row": 1, "level": 0, "switch": "door"}]
        return normalize_map(data)

    def test_rotation_moves_attached_light_directions_faces_and_multilevel_shapes(self):
        block = self.block()
        original = copy.deepcopy(block)
        result, additions = transform_block(block, "rotate", {})
        self.assertEqual(block, original)
        self.assertEqual(additions, {})
        self.assertEqual((result["grid_cols"], result["grid_rows"]), (4, 5))
        light = result["levels"][0]["lights"][0]
        wall = result["levels"][0]["walls"][0]
        self.assertEqual((light["col"], light["row"], light["side"]), (2, 1, "E"))
        self.assertEqual(
            wall_endpoints_for_cell_side(light["col"], light["row"], light["side"]),
            tuple(wall[key] for key in ("c0", "r0", "c1", "r1")),
        )
        self.assertEqual(
            (wall["east"], wall["south"], wall["top"]), ("north-material", "east-material", "top-material")
        )
        self.assertEqual(result["ramps"][0]["low"], [2, 1])
        self.assertEqual(result["ramps"][0]["high"], [1, 4])
        self.assertEqual(result["ladders"][0]["side"], "N")
        zone = result["actor_spawn_zones"][0]
        self.assertEqual((zone["cols"], zone["rows"], zone["levels"], zone["count"]), ([3, 4], [0, 2], 2, [2, 4]))

    def test_four_rotations_or_two_reflections_restore_every_record(self):
        original = self.block()
        for operation, times in [("rotate", 4), ("mirror_x", 2), ("mirror_y", 2)]:
            result = original
            for _ in range(times):
                result, _ = transform_block(result, operation, {})
            self.assertEqual(result, original, operation)

    def test_nested_geometry_is_copied_and_motion_nudges_rotate_with_it(self):
        room = empty_map(2, 1)
        room["player_spawn_zones"] = []
        room["levels"][0]["floors"] = [floor(0, 0)]
        block = empty_map(6, 4)
        block["player_spawn_zones"] = []
        entry = nested("room", 0, [1, 1], [3, 2])
        entry["from_nudge"] = [0.5, 2.0, -0.25]
        block["nested_maps"] = [entry]
        definitions = {"room": room}
        snapshot = copy.deepcopy(definitions)
        result, additions = transform_block(block, "rotate", definitions)
        self.assertEqual(definitions, snapshot)
        transformed = result["nested_maps"][0]
        self.assertEqual((transformed["from"], transformed["to"]), ([2, 1], [1, 3]))
        self.assertEqual(transformed["from_nudge"], [0.25, 2.0, 0.5])
        self.assertEqual(transformed["travel_secs"], entry["travel_secs"])
        self.assertNotEqual(transformed["map"], "room")
        child = additions[transformed["map"]]
        self.assertEqual((child["grid_cols"], child["grid_rows"]), (1, 2))
        self.assertEqual((child["levels"][0]["floors"][0]["col"], child["levels"][0]["floors"][0]["row"]), (0, 0))

    def test_nested_references_share_one_transformed_copy_and_reject_partial_footprints(self):
        room = empty_map(2, 1)
        block = empty_map(6, 4)
        block["nested_maps"] = [nested("room", 0, [0, 0], [0, 0]), nested("room", 0, [3, 2], [3, 2])]
        result, additions = transform_block(block, "mirror_x", {"room": room})
        self.assertEqual(len(additions), 1)
        self.assertEqual(result["nested_maps"][0]["map"], result["nested_maps"][1]["map"])
        block["nested_maps"][1]["from"] = [5, 2]
        before = copy.deepcopy(block)
        with self.assertRaisesRegex(ValueError, "full nested-map footprints"):
            transform_block(block, "rotate", {"room": room})
        self.assertEqual(block, before)

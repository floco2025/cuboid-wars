import copy
import unittest

from editor_fixtures import DEFAULT_ALIAS, faces, floor, nested
from map_editor.normalization import (
    canonicalize_map,
    empty_level,
    empty_map,
    nested_map_spans_level,
    normalize_map,
    normalize_nested_map,
)
from map_editor.validation import validate_map

KIND = "treasure"
BRIDGE_KIND = "skyway"


class NormalizationTests(unittest.TestCase):
    def test_canonicalization_deduplicates_edges_and_applies_ramp_floor_rules(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append({**empty_level(1), "floors": [floor(0, 0), floor(1, 0)]})
        data["levels"][0]["walls"] = [
            {"c0": 1, "r0": 1, "c1": 0, "r1": 1, **faces()},
            {"c0": 0, "r0": 1, "c1": 1, "r1": 1, **faces()},
        ]
        data["ramps"] = [{"lower_level": 0, "low": [0, 0], "high": [2, 1], **faces()}]

        result = canonicalize_map(data)

        self.assertEqual(len(result["levels"][0]["walls"]), 1)
        self.assertEqual(
            {(entry["col"], entry["row"]) for entry in result["levels"][0]["floors"]},
            {(0, 0), (1, 0)},
        )
        self.assertEqual(result["levels"][1]["floors"], [])

    def test_canonicalization_of_stacked_ramps_is_a_fixed_point(self) -> None:
        data = empty_map(6, 6)
        data["levels"] = [copy.deepcopy(data["levels"][0]) for _ in range(3)]
        data["ramps"] = [
            {"lower_level": 1, "low": [1, 1], "high": [3, 1], "all": DEFAULT_ALIAS},
            {"lower_level": 0, "low": [1, 1], "high": [3, 1], "all": DEFAULT_ALIAS},
        ]
        once = canonicalize_map(data)
        self.assertEqual(canonicalize_map(once), once)

    def test_canonicalization_preserves_conflicting_plate_switches_for_validation(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        barrier = {"level": 0, "col": 0, "row": 0, "switch": KIND}
        firework = {"level": 0, "col": 0, "row": 0, "switch": "fireworks"}
        data["pressure_plates"] = [firework, barrier, dict(firework)]

        result = canonicalize_map(data)

        self.assertEqual(result["pressure_plates"], [firework, barrier])
        errors = validate_map(result, [KIND], [], switches=[KIND, "fireworks"])
        self.assertTrue(any("duplicates a plate at level 0 [0, 0]" in error for error in errors))

    def test_canonicalization_keeps_actor_zones_that_differ_only_by_switch(self) -> None:
        data = empty_map(4, 4)
        zone = {"level": 0, "cols": [0, 2], "rows": [0, 2], "kind": "zapper", "count": 1}
        data["actor_spawn_zones"] = [{**zone, "switch": "guards"}, dict(zone), {**zone, "switch": "guards"}, dict(zone)]

        result = canonicalize_map(data)

        self.assertEqual(result["actor_spawn_zones"], [zone, {**zone, "switch": "guards"}])

    def test_zone_and_nested_map_switches_survive_normalization_only_when_set(self) -> None:
        data = empty_map(2, 2)
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 1, "switch": "guards"},
            {"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "zapper", "count": 1, "switch": ""},
        ]
        data["nested_maps"] = [
            {"map": "tile", "level": 0, "from": [0, 0], "to": [1, 0], "switch": "lift"},
            {"map": "tile", "level": 0, "from": [1, 1], "to": [1, 1], "switch": None},
        ]

        result = normalize_map(data)

        self.assertEqual([zone.get("switch") for zone in result["actor_spawn_zones"]], ["guards", None])
        self.assertEqual([entry.get("switch") for entry in result["nested_maps"]], ["lift", None])
        self.assertNotIn("switch", result["actor_spawn_zones"][1])
        self.assertNotIn("switch", result["nested_maps"][1])

    def test_canonicalization_keeps_the_last_bridge_per_cell_sorted_by_row_then_col(self) -> None:
        data = empty_map(3, 3)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["light_bridges"] = [
            {"col": 2, "row": 1, "kind": BRIDGE_KIND},
            {"col": 1, "row": 0, "kind": BRIDGE_KIND},
            {"col": 2, "row": 1, "kind": "other"},
            {"col": 0, "row": 1, "kind": BRIDGE_KIND},
        ]

        result = canonicalize_map(data)

        self.assertEqual(
            result["levels"][0]["light_bridges"],
            [
                {"col": 1, "row": 0, "kind": BRIDGE_KIND},
                {"col": 0, "row": 1, "kind": BRIDGE_KIND},
                {"col": 2, "row": 1, "kind": "other"},
            ],
        )

    def test_canonicalization_sorts_and_drops_out_of_range_nested_maps(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [
            nested("cabin", 0, [3, 3], [3, 3]),
            nested("cabin", 0, [1, 1], [1, 1]),
            nested("cabin", 0, [6, 1], [1, 1]),
            nested("cabin", 2, [1, 1], [1, 1]),
            nested("bad/name", 0, [2, 2], [2, 2]),
        ]
        self.assertEqual([e["from"] for e in canonicalize_map(data)["nested_maps"]], [[1, 1], [3, 3]])

    def test_nested_map_nudges_default_to_zero(self) -> None:
        entry = normalize_nested_map({"map": "cabin", "level": 0, "from": [1, 1], "to": [2, 1]})
        self.assertEqual((entry["from_nudge"], entry["to_nudge"]), ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]))
        self.assertEqual(entry["travel_secs"], 2.0)

    def test_a_nested_map_spans_its_own_storeys_plus_its_motion(self) -> None:
        lift = nested("cabin", 1, [1, 1], [1, 1], 2)
        self.assertFalse(nested_map_spans_level(lift, 0, 2))
        self.assertTrue(nested_map_spans_level(lift, 1, 2))
        self.assertTrue(nested_map_spans_level(lift, 3, 2))
        self.assertFalse(nested_map_spans_level(lift, 4, 2))

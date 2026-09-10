import unittest

from editor_fixtures import DEFAULT_ALIAS, EditorHost, NESTED_SHAPES, faces, floor, nested
from map_editor.editing import paint_floors
from map_editor.normalization import empty_level, empty_map
from map_editor.catalogs import MapCatalogs
from map_editor.validation import placed_definitions, validate_document, validate_map

KIND = "treasure"
BRIDGE_KIND = "skyway"


class ValidationTests(unittest.TestCase):
    def test_one_tile_map_has_an_in_bounds_spawn_zone(self) -> None:
        self.assertEqual(validate_map(empty_map(1, 1), [], []), [])

    def test_valid_minimal_map_has_no_errors(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        self.assertEqual(validate_map(data, [], []), [])

    def test_invalid_geometry_item_and_ladder_are_reported(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["walls"] = [{"c0": 0, "r0": 0, "c1": 2, "r1": 0, **faces()}]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "gold"}]
        data["ladders"] = [{"lower_level": 0, "col": 0, "row": 0, "side": "N", "levels": 1}]

        errors = validate_map(data, [], [])

        self.assertTrue(any("is not one grid edge" in error for error in errors))
        self.assertTrue(any("has no regular floor" in error for error in errors))
        self.assertTrue(any("but the map has 1 level(s)" in error for error in errors))

    def test_material_validation_uses_the_supplied_catalog(self) -> None:
        data = paint_floors(empty_map(), 0, (3, 3, 4, 4), "fresh_alias")
        self.assertFalse(validate_map(data, [], [], material_aliases=["fresh_alias"]))
        self.assertTrue(validate_map(data, [], [], material_aliases=[DEFAULT_ALIAS]))


class BarrierKindTests(unittest.TestCase):
    def test_unlisted_kind_is_flagged_naming_the_listed_ones(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "nope"}]
        data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "key", "kind": "nope"}]

        errors = validate_map(data, [KIND, "lobby"], [])

        self.assertTrue(any("barrier[0] has unknown kind 'nope'; known: [treasure, lobby]" in e for e in errors))
        self.assertTrue(any("unknown key kind 'nope'; known: [treasure, lobby]" in e for e in errors))

        errors = validate_map(data, [], [])
        self.assertTrue(any("known: [(none listed)]" in e for e in errors))


class PressurePlateTests(unittest.TestCase):
    def test_plate_validation_flags_missing_and_unknown_switches(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 1)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0},
            {"level": 0, "col": 0, "row": 0, "switch": "nope"},
            {"level": 0, "col": 1, "row": 1, "switch": "fireworks"},
            {"level": 0, "col": 1, "row": 1, "switch": "fireworks"},
        ]

        errors = validate_map(data, [KIND], [], switches=[KIND, "fireworks"])

        self.assertTrue(any("pressure_plates[0] has no switch" in error for error in errors))
        self.assertTrue(any("unknown switch 'nope'; known: [treasure, fireworks]" in error for error in errors))
        self.assertTrue(any("duplicates a plate" in error for error in errors))
        self.assertFalse(any("unknown switch" in error for error in validate_map(data, [KIND], [])))

    def test_zone_and_nested_map_switches_must_be_known_and_plated(self) -> None:
        data = empty_map(4, 4)
        data["levels"][0]["floors"] = [floor(0, 0), floor(2, 2)]
        data["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "switch": "guards"}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [2, 3], "rows": [2, 3], "kind": "zapper", "count": 1, "switch": "guards"},
            {"level": 0, "cols": [2, 3], "rows": [2, 3], "kind": "zapper", "count": 1, "switch": "nope"},
            {"level": 0, "cols": [2, 3], "rows": [2, 3], "kind": "zapper", "count": 1, "switch": "lift"},
            {"level": 0, "cols": [2, 3], "rows": [2, 3], "kind": "zapper", "count": 1, "switch": ""},
        ]
        data["nested_maps"] = [
            {**nested("cabin", 0, [1, 1], [3, 1]), "switch": "guards"},
            {**nested("cabin", 0, [1, 2], [3, 2]), "switch": "lift"},
        ]
        switches = ["guards", "lift"]

        errors = validate_map(data, [], [], switches=switches, plated_switches={"guards"})

        self.assertTrue(any("actor_spawn_zones[1] names unknown switch 'nope'" in e for e in errors))
        self.assertTrue(any("actor_spawn_zones[2] names switch 'lift', which no pressure plate operates" in e for e in errors))
        self.assertTrue(any("actor_spawn_zones[3] has an empty switch" in e for e in errors))
        self.assertTrue(any("nested_maps[1] names switch 'lift', which no pressure plate operates" in e for e in errors))
        self.assertFalse(any("actor_spawn_zones[0]" in e or "nested_maps[0]" in e for e in errors))
        self.assertEqual(
            [e for e in validate_map(data, [], []) if "switch" in e],
            ["actor_spawn_zones[3] has an empty switch"],
            "without a catalog only the empty switch is an error",
        )

    def test_only_placed_geometry_supplies_a_targets_plates(self) -> None:
        data = empty_map(6, 6)
        data["levels"][0]["floors"] = [floor(0, 0), floor(3, 3)]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [3, 4], "rows": [3, 4], "kind": "zapper", "count": 1, "switch": "guards"},
        ]
        room = empty_map(1, 1)
        room["levels"][0]["floors"] = [floor(0, 0)]
        room["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "switch": "guards"}]
        data["nested_geometry"] = {"room": room, "hall": empty_map(2, 2)}
        catalogs = MapCatalogs({}, {}, 0.1, {DEFAULT_ALIAS: True}, ["guards"])

        self.assertEqual(placed_definitions(data, data["nested_geometry"]), {})
        errors = validate_document(data, catalogs, actor_kinds=["zapper"])
        self.assertTrue(any("names switch 'guards', which no pressure plate operates" in e for e in errors))

        data["nested_geometry"]["hall"]["nested_maps"] = [nested("room", 0, [0, 0], [0, 0])]
        data["nested_maps"] = [nested("hall", 0, [1, 1], [1, 1])]
        self.assertEqual(set(placed_definitions(data, data["nested_geometry"])), {"hall", "room"})
        errors = validate_document(data, catalogs, actor_kinds=["zapper"])
        self.assertFalse(any("no pressure plate operates" in e for e in errors), list(errors))

    def test_plates_need_a_slab_outside_ramp_footprints(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(empty_level(1))
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 1)]
        data["levels"][0]["inaccessible_floors"] = [floor(3, 3)]
        data["ramps"] = [{"lower_level": 0, "low": [0, 1], "high": [2, 2], **faces()}]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "switch": "fireworks"},
            {"level": 0, "col": 3, "row": 3, "switch": "fireworks"},
            {"level": 0, "col": 1, "row": 0, "switch": "fireworks"},
            {"level": 0, "col": 1, "row": 1, "switch": "fireworks"},
        ]

        self.assertEqual(
            validate_map(data, [], []),
            ["pressure_plates[2] [1, 0] has no floor", "pressure_plates[3] [1, 1] is inside a ramp footprint"],
        )
        host = EditorHost(data, [])
        host.add_pressure_plate(1, 0, "fireworks")
        self.assertEqual(host.statuses, ["Plate not placed: cell [1, 0] has no floor."])
        self.assertEqual(len(host.map_data["pressure_plates"]), 4)


class LightBridgeTests(unittest.TestCase):
    def test_bridge_validation_flags_kinds_cells_and_plate_conflicts(self) -> None:
        data = empty_map(3, 3)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["inaccessible_floors"] = [floor(1, 0)]
        data["levels"].append({**empty_level(1), "floors": [floor(2, 2)]})
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        data["levels"][0]["light_bridges"] = [
            {"col": 0, "row": 0, "kind": "nope"},
            {"col": 1, "row": 0, "kind": BRIDGE_KIND},
            {"col": 1, "row": 1, "kind": BRIDGE_KIND},
            {"col": 2, "row": 2, "kind": BRIDGE_KIND},
            {"col": 2, "row": 2, "kind": BRIDGE_KIND},
        ]
        data["pressure_plates"] = [
            {"level": 0, "col": 2, "row": 2, "switch": "fireworks"},
            {"level": 0, "col": 0, "row": 0, "switch": "nope"},
        ]

        errors = validate_map(data, [], [BRIDGE_KIND], switches=["fireworks", BRIDGE_KIND])

        self.assertTrue(any("light_bridge[0] has unknown kind 'nope'; known: [skyway]" in e for e in errors))
        self.assertTrue(any("light_bridge[0] [0, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("light_bridge[1] [1, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("light_bridge[2] [1, 1] sits on a ramp" in e for e in errors))
        self.assertTrue(any("light_bridge[4] [2, 2] duplicates another light bridge" in e for e in errors))
        self.assertTrue(any("pressure_plates[0] [2, 2] sits on a light bridge" in e for e in errors))
        self.assertTrue(any("pressure_plates[1] has unknown switch 'nope'; known: [fireworks, skyway]" in e for e in errors))


class NestedMapTests(unittest.TestCase):
    def test_nested_map_validation_flags_missing_files_cycles_self_nesting_bounds_and_timing(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [
            nested("ghost", 0, [1, 1], [1, 1]),
            nested("loop_a", 0, [2, 2], [2, 2]),
            nested("home", 0, [3, 3], [3, 3]),
            {**nested("cabin", 0, [4, 4], [7, 4]), "travel_secs": 0.0, "phase_secs": -1.0, "to_nudge": [1.0, 2.0]},
            nested("cabin", 0, [4, 4], [4, 4], 3),
        ]
        errors = validate_map(data, [], [], map_name="home", nested_lookup=NESTED_SHAPES.get)
        self.assertTrue(any("ghost" in error and "missing" in error for error in errors))
        self.assertTrue(any("nested maps loop" in error and "loop_a -> loop_b -> loop_a" in error for error in errors))
        self.assertTrue(any("nests the edited map itself" in error for error in errors))
        self.assertTrue(any("is outside the grid" in error for error in errors))
        self.assertTrue(any("positive travel time" in error for error in errors))
        self.assertTrue(any("negative pause or phase" in error for error in errors))
        self.assertTrue(any("to_nudge is not three numbers" in error for error in errors))
        self.assertTrue(any("but the map has 1 level(s)" in error for error in errors))
        self.assertTrue(any("duplicates a nested map" in error for error in errors))

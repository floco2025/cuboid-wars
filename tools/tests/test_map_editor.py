import json
import tempfile
import unittest
from unittest.mock import Mock
from pathlib import Path

from PySide6.QtCore import QPointF

from map_editor.constants import (
    DEFAULT_ALIAS,
    FACES,
    MODES,
    MODE_ERASE_BARRIERS,
    MODE_ERASE_FLOORS,
    MODE_ERASE_NESTED_MAPS,
    MODE_ERASE_RAMPS,
    MODE_ERASE_SPAWN_ZONES,
    MODE_ERASE_WALLS,
    MODE_LIGHT_BRIDGE,
    MODE_SELECT,
    load_map_barrier_kinds,
    load_map_bridge_kinds,
)
from map_editor.erase import EraseMixin
from map_editor.items import ItemsMixin
from map_editor.lights import LightsMixin
from map_editor.select import SelectMixin
from map_editor.spawn_zones import SpawnZoneEditMixin
from map_editor.types import ZoneRef
from map_editor.geometry import ramp_axis, ramp_cells, wall_segments_between
from map_editor.io import empty_map, read_map, write_map
from map_editor.nested_maps import (
    NestedMapShape,
    NestedMapsMixin,
    nested_map_cycle,
    nested_map_label,
    nested_map_rest_points,
)
from map_editor.normalization import canonicalize_map, normalize_nested_map
from map_editor.transforms import resize_map_data
from map_editor.placement import PlacementMixin
from map_editor.structure import insert_level_data, remove_level_data
from map_editor.validation import validate_map


def faces(alias: str = DEFAULT_ALIAS) -> dict[str, str]:
    return {face: alias for face in FACES}


def floor(col: int, row: int) -> dict:
    return {"col": col, "row": row, **faces()}


KIND = "treasure"
BRIDGE_KIND = "skyway"


def upper_level(*floors: dict) -> dict:
    return {
        "name": "Upper",
        "floors": list(floors),
        "inaccessible_floors": [],
        "grass": [],
        "walls": [],
        "barriers": [],
        "lights": [],
    }


# Stand-in map files for nested-map tests: `cabin` is a 3x2 room on two
# storeys, `loop_a` and `loop_b` nest each other.
NESTED_SHAPES = {
    "cabin": NestedMapShape(grid_cols=3, grid_rows=2, level_count=2, nested_names=()),
    "loop_a": NestedMapShape(grid_cols=1, grid_rows=1, level_count=1, nested_names=("loop_b",)),
    "loop_b": NestedMapShape(grid_cols=1, grid_rows=1, level_count=1, nested_names=("loop_a",)),
}


class StubCanvas:
    def update(self) -> None:
        pass

    def cells_per_pixel(self, pixels: float) -> float:
        return pixels / 36.0


class EditorHost(PlacementMixin, ItemsMixin, LightsMixin, NestedMapsMixin, EraseMixin, SelectMixin, SpawnZoneEditMixin):
    def __init__(self, map_data: dict, bridge_kinds: list[str]) -> None:
        self.map_data = map_data
        self.current_level = 0
        self.bridge_kinds = bridge_kinds
        self.barrier_kinds = ["barrier_1"]
        self.canvas = StubCanvas()
        self.spawn_zone_drag = None
        self.current_material = DEFAULT_ALIAS
        self.selected_spawn_zone_ref = None
        self.tile_selection = None
        self.select_drag_kind = None
        self.statuses: list[str] = []
        self.path = None
        self.recent_nested_map = None

    def nested_map_shape(self, name: str) -> NestedMapShape | None:
        return NESTED_SHAPES.get(name)

    def apply_change(self, label: str, after: dict) -> None:
        self.map_data = after

    def notify(self, message: str) -> None:
        self.statuses.append(message)

    def update_selection_actions(self) -> None:
        pass


class GeometryTests(unittest.TestCase):
    def test_wall_segments_are_unit_length_and_canonical(self) -> None:
        self.assertEqual(
            wall_segments_between((3, 2), (0, 2)),
            [[2, 2, 3, 2], [1, 2, 2, 2], [0, 2, 1, 2]],
        )

    def test_ramp_cells_and_axis_follow_its_footprint(self) -> None:
        ramp = {"low": [3, 1], "high": [0, 3]}
        self.assertEqual(ramp_axis(ramp), "west")
        self.assertEqual(ramp_cells(ramp), {(0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)})


class NormalizationTests(unittest.TestCase):
    def test_canonicalization_deduplicates_edges_and_applies_ramp_floor_rules(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(upper_level(floor(0, 0), floor(1, 0)))
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


class FileIoTests(unittest.TestCase):
    def test_map_files_are_written_without_a_schema_version(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)

            wrapper = json.loads(path.read_text(encoding="utf-8"))
            self.assertNotIn("version", wrapper)
            self.assertNotIn("barrier_kinds", wrapper["map"])
            self.assertEqual(read_map(path), canonicalize_map(data))


class ResizeTests(unittest.TestCase):
    def test_center_resize_translates_every_coordinate_family(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(upper_level())
        level = data["levels"][0]
        level["floors"] = [floor(1, 1)]
        level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, **faces()}]
        level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        level["light_bridges"] = [{"col": 1, "row": 3, "kind": BRIDGE_KIND}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 3], "rows": [1, 3], "kind": "mine", "count": 1}
        ]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "cookie"}]
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


class PressurePlateTests(unittest.TestCase):
    def test_canonicalization_keeps_one_plate_per_type_on_a_cell(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        barrier = {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": KIND}
        firework = {"level": 0, "col": 0, "row": 0, "type": "firework"}
        data["pressure_plates"] = [firework, barrier, dict(firework)]

        result = canonicalize_map(data)

        self.assertEqual(result["pressure_plates"], [barrier, firework])

    def test_plates_round_trip_through_the_file_format(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": KIND},
            {"level": 0, "col": 1, "row": 0, "type": "firework"},
        ]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"type": "firework"}', text)
            self.assertEqual(read_map(path)["pressure_plates"], data["pressure_plates"])

    def test_plate_validation_flags_bad_types_and_kinds(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "type": "confetti"},
            {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": "nope"},
            {"level": 0, "col": 1, "row": 0, "type": "firework", "kind": KIND},
            {"level": 0, "col": 1, "row": 1, "type": "firework"},
            {"level": 0, "col": 1, "row": 1, "type": "firework"},
        ]

        errors = validate_map(data, [KIND], [])

        self.assertTrue(any("unknown type 'confetti'" in error for error in errors))
        self.assertTrue(any("unknown barrier kind 'nope'; known: [treasure]" in error for error in errors))
        self.assertTrue(any("must not have `kind`" in error for error in errors))
        self.assertTrue(any("duplicates a plate" in error for error in errors))


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

    def test_shipped_kinds_are_loaded_from_gameplay_settings(self) -> None:
        self.assertEqual(
            load_map_barrier_kinds("hotel"),
            {"treasure": "#ff3333", "basement": "#f0c020", "gravity": "#5090ff", "lobby": "#22cc33"},
        )
        self.assertEqual(load_map_barrier_kinds("obby"), {"barrier_1": "#f0c020"})
        self.assertEqual(load_map_barrier_kinds("not_configured"), {})


class LightBridgeTests(unittest.TestCase):
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

    def test_placing_a_bridge_rect_covers_every_dragged_cell(self) -> None:
        data = empty_map(3, 3)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["inaccessible_floors"] = [floor(1, 0)]
        data["levels"].append(upper_level())
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        host = EditorHost(data, [BRIDGE_KIND])

        host.add_light_bridge_rect((0, 0), (2, 1), BRIDGE_KIND)

        self.assertEqual(
            {(b["col"], b["row"], b["kind"]) for b in host.map_data["levels"][0]["light_bridges"]},
            {(col, row, BRIDGE_KIND) for row in range(2) for col in range(3)},
        )
        self.assertEqual(host.statuses, [])
        errors = validate_map(host.map_data, [], [BRIDGE_KIND])
        self.assertTrue(any("[0, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("[1, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("[1, 1] sits on a ramp" in e for e in errors))

    def test_erase_keep_floors_leaves_bridges_in_place(self) -> None:
        data = empty_map(3, 3)
        level = data["levels"][0]
        level["floors"] = [floor(0, 0)]
        level["light_bridges"] = [{"col": 1, "row": 0, "kind": BRIDGE_KIND}]
        level["walls"] = [{"c0": 1, "r0": 0, "c1": 2, "r1": 0, **faces()}]
        host = EditorHost(data, [BRIDGE_KIND])

        host.erase_cell_rect((0, 0), (2, 2), preserve_floors=True)

        level = host.map_data["levels"][0]
        self.assertEqual(level["walls"], [])
        self.assertEqual(level["floors"], [floor(0, 0)])
        self.assertEqual(level["light_bridges"], [{"col": 1, "row": 0, "kind": BRIDGE_KIND}])

        bridge_center = QPointF(1.5, 0.5)
        self.assertEqual(host.hit_at(bridge_center), (MODE_LIGHT_BRIDGE, (1, 0)))
        host.erase_at(bridge_center, preserve_floors=True)
        self.assertEqual(host.map_data["levels"][0]["light_bridges"], [{"col": 1, "row": 0, "kind": BRIDGE_KIND}])

        host.erase_at(bridge_center, preserve_floors=False)
        self.assertEqual(host.map_data["levels"][0]["light_bridges"], [])

    def test_bridges_and_bridge_plates_round_trip_through_the_file_format(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["light_bridges"] = [{"col": 1, "row": 0, "kind": BRIDGE_KIND}]
        data["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "type": "bridge", "kind": BRIDGE_KIND}]
        self.assertEqual(validate_map(data, [], [BRIDGE_KIND]), [])

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"light_bridges": [', text)
            self.assertIn('"type": "bridge"', text)
            loaded = read_map(path)
            self.assertEqual(loaded["levels"][0]["light_bridges"], data["levels"][0]["light_bridges"])
            self.assertEqual(loaded["pressure_plates"], data["pressure_plates"])

    def test_bridge_validation_flags_kinds_cells_and_plate_conflicts(self) -> None:
        data = empty_map(3, 3)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["inaccessible_floors"] = [floor(1, 0)]
        data["levels"].append(upper_level(floor(2, 2)))
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        data["levels"][0]["light_bridges"] = [
            {"col": 0, "row": 0, "kind": "nope"},
            {"col": 1, "row": 0, "kind": BRIDGE_KIND},
            {"col": 1, "row": 1, "kind": BRIDGE_KIND},
            {"col": 2, "row": 2, "kind": BRIDGE_KIND},
            {"col": 2, "row": 2, "kind": BRIDGE_KIND},
        ]
        data["pressure_plates"] = [
            {"level": 0, "col": 2, "row": 2, "type": "firework"},
            {"level": 0, "col": 0, "row": 0, "type": "bridge", "kind": "nope"},
        ]

        errors = validate_map(data, [], [BRIDGE_KIND])

        self.assertTrue(any("light_bridge[0] has unknown kind 'nope'; known: [skyway]" in e for e in errors))
        self.assertTrue(any("light_bridge[0] [0, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("light_bridge[1] [1, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("light_bridge[2] [1, 1] sits on a ramp" in e for e in errors))
        self.assertTrue(any("light_bridge[4] [2, 2] duplicates another light bridge" in e for e in errors))
        self.assertTrue(any("pressure_plates[0] [2, 2] sits on a light bridge" in e for e in errors))
        self.assertTrue(any("unknown bridge kind 'nope'; known: [skyway]" in e for e in errors))

    def test_shipped_bridge_kinds_are_loaded_from_gameplay_settings(self) -> None:
        self.assertEqual(load_map_bridge_kinds("hotel"), {})
        self.assertEqual(
            load_map_bridge_kinds("obby"),
            {"bridge_1": "#30d8ff", "bridge_2": "#30d8ff", "bridge_3": "#30d8ff"},
        )
        self.assertEqual(load_map_bridge_kinds("not_configured"), {})


def wall(c0: int, r0: int, c1: int, r1: int) -> dict:
    return {"c0": c0, "r0": r0, "c1": c1, "r1": r1, **faces()}


def actor_zone(level: int, c0: int, r0: int, c1: int, r1: int) -> dict:
    return {"level": level, "cols": [c0, c1], "rows": [r0, r1], "kind": "sentry", "count": 1}


class LayerEraserTests(unittest.TestCase):
    """Each element group's eraser clears only its own element; Erase clears every element."""

    def host(self) -> EditorHost:
        data = empty_map(4, 4)
        data["levels"].append(upper_level(floor(0, 0), floor(1, 0)))
        level = data["levels"][0]
        level["floors"] = [floor(0, 0), floor(1, 0), floor(3, 3)]
        level["inaccessible_floors"] = [floor(0, 1)]
        level["walls"] = [wall(0, 0, 1, 0), wall(3, 3, 4, 3)]
        level["barriers"] = [{**wall(1, 0, 1, 1), "kind": KIND}]
        data["ramps"] = [
            {"low": [0, 2], "high": [1, 4], "lower_level": 0, **faces()},
            {"low": [2, 0], "high": [3, 1], "lower_level": 1, **faces()},
        ]
        data["actor_spawn_zones"] = [actor_zone(0, 0, 0, 2, 2), actor_zone(1, 0, 0, 2, 2)]
        data["player_spawn_zones"] = [{"level": 0, "cols": [3, 4], "rows": [3, 4]}]
        data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "cookie"}]
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 0, "type": "firework"}]
        return EditorHost(data, [BRIDGE_KIND])

    def test_erase_floors_removes_only_floors_in_the_rectangle(self) -> None:
        host = self.host()
        host.erase_group_rect(MODE_ERASE_FLOORS, (0, 0), (1, 1))
        level = host.map_data["levels"][0]
        self.assertEqual(level["floors"], [floor(3, 3)])
        self.assertEqual(level["inaccessible_floors"], [])
        self.assertEqual(len(level["walls"]), 2)
        self.assertEqual(len(level["barriers"]), 1)

    def test_erase_walls_and_erase_barriers_leave_each_other_alone(self) -> None:
        host = self.host()
        host.erase_group_rect(MODE_ERASE_WALLS, (0, 0), (1, 1))
        level = host.map_data["levels"][0]
        self.assertEqual(level["walls"], [wall(3, 3, 4, 3)])
        self.assertEqual(len(level["barriers"]), 1)

        host.erase_group_rect(MODE_ERASE_BARRIERS, (0, 0), (1, 1))
        level = host.map_data["levels"][0]
        self.assertEqual(level["barriers"], [])
        self.assertEqual(level["walls"], [wall(3, 3, 4, 3)])
        self.assertEqual(level["floors"], [floor(0, 0), floor(1, 0), floor(3, 3)])

    def test_erase_ramps_touches_only_ramps_on_the_current_level(self) -> None:
        host = self.host()
        host.erase_group_rect(MODE_ERASE_RAMPS, (0, 0), (3, 3))
        self.assertEqual([ramp["lower_level"] for ramp in host.map_data["ramps"]], [1])

    def test_erase_spawn_zones_clears_both_zone_lists_on_the_current_level(self) -> None:
        host = self.host()
        host.selected_spawn_zone_ref = object()
        host.erase_group_rect(MODE_ERASE_SPAWN_ZONES, (1, 1), (3, 3))
        self.assertEqual(host.map_data["actor_spawn_zones"], [actor_zone(1, 0, 0, 2, 2)])
        self.assertEqual(host.map_data["player_spawn_zones"], [])
        self.assertIsNone(host.selected_spawn_zone_ref)

    def test_an_empty_selection_flashes_and_changes_nothing(self) -> None:
        host = self.host()
        before = json.dumps(host.map_data, sort_keys=True)
        host.erase_group_rect(MODE_ERASE_WALLS, (2, 1), (2, 2))
        host.erase_group_rect(MODE_ERASE_RAMPS, (3, 0), (3, 1))
        self.assertEqual(json.dumps(host.map_data, sort_keys=True), before)
        self.assertEqual(
            host.statuses,
            ["Erase Walls: no walls in selection.", "Erase Ramps: no ramps in selection."],
        )

    def test_erase_clears_every_element_and_keep_floors_keeps_what_stands_on_them(self) -> None:
        host = self.host()
        host.erase_cell_rect((0, 0), (1, 1), preserve_floors=True)
        level = host.map_data["levels"][0]
        self.assertEqual(level["floors"], [floor(0, 0), floor(1, 0), floor(3, 3)])
        self.assertEqual(level["walls"], [wall(3, 3, 4, 3)])
        self.assertEqual(level["barriers"], [])
        self.assertEqual(len(host.map_data["items"]), 1)
        self.assertEqual(len(host.map_data["pressure_plates"]), 1)

        host.erase_cell_rect((0, 0), (1, 1), preserve_floors=False)
        level = host.map_data["levels"][0]
        self.assertEqual(level["floors"], [floor(3, 3)])
        self.assertEqual(host.map_data["items"], [])
        self.assertEqual(host.map_data["pressure_plates"], [])


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
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "cookie"}]
        data["ladders"] = [{"lower_level": 0, "col": 0, "row": 0, "side": "N", "levels": 1}]

        errors = validate_map(data, [], [])

        self.assertTrue(any("is not one grid edge" in error for error in errors))
        self.assertTrue(any("has no regular floor" in error for error in errors))
        self.assertTrue(any("but the map has 1 level(s)" in error for error in errors))


class ModeTests(unittest.TestCase):
    def test_selection_comes_first_and_has_its_own_drag_handler(self) -> None:
        from map_editor.canvas import RELEASE_TOOLS

        self.assertEqual(MODES[0], MODE_SELECT)
        self.assertNotIn(MODE_SELECT, RELEASE_TOOLS)


class RightClickAndSelectTests(unittest.TestCase):
    CELL = 10.0

    def furnished(self) -> EditorHost:
        data = empty_map(8, 8)
        level = data["levels"][0]
        level["floors"] = [floor(1, 1), floor(2, 2)]
        level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "all": DEFAULT_ALIAS}]
        level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "type": "barrier", "kind": "barrier_1"}]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "cookie"}]
        data["player_spawn_zones"] = []
        return EditorHost(data, ["bridge_1"])

    def test_right_click_peels_the_light_then_plate_item_and_floor_off_a_cell(self) -> None:
        host = self.furnished()
        near_top = QPointF(1.5, 1.05)
        center = QPointF(1.5, 1.5)

        self.assertEqual(host.hit_at(near_top), ("Light", (1, 1, "N")))
        host.erase_hit(("Light", (1, 1, "N")))
        self.assertEqual(host.map_data["levels"][0]["lights"], [])
        self.assertEqual(host.hit_at(near_top)[0], "Wall")

        self.assertEqual(host.hit_at(center), ("Pressure Plate", (1, 1)))
        self.assertEqual([p["type"] for p in host.plates_at(1, 1)], ["barrier"])
        host.erase_hit(("Pressure Plate", (1, 1)))
        self.assertEqual(host.map_data["pressure_plates"], [])

        self.assertEqual(host.hit_at(center), ("Item", (1, 1)))
        host.erase_hit(("Item", (1, 1)))
        self.assertEqual(host.map_data["items"], [])
        self.assertEqual(host.hit_at(center), ("Floor", (1, 1)))

    def test_placing_on_an_occupied_cell_flashes_instead_of_removing(self) -> None:
        host = self.furnished()
        host.prompt_and_add_item(1, 1)
        self.assertTrue(host.statuses[-1].startswith("Item not placed"))
        host.add_pressure_plate(1, 1, "barrier_1")
        self.assertTrue(host.statuses[-1].startswith("Plate not placed"))
        host.add_light_at(QPointF(1.5, 1.05))
        self.assertIn("already a light", host.statuses[-1])
        self.assertEqual(len(host.map_data["items"]), 1)
        self.assertEqual(len(host.map_data["pressure_plates"]), 1)
        self.assertEqual(len(host.map_data["levels"][0]["lights"]), 1)

    def test_a_press_selects_a_spawn_zone_before_a_drag_can_move_it(self) -> None:
        data = empty_map(8, 8)
        data["actor_spawn_zones"] = [{"level": 0, "cols": [1, 3], "rows": [1, 3], "kind": "beetle", "count": 2}]
        host = EditorHost(data, [])
        inside = QPointF(2.5, 2.5)

        self.assertFalse(host.begin_select_press(inside, edit_objects=True))
        self.assertEqual(host.selected_spawn_zone_ref, ZoneRef("actor_spawn_zones", 0))
        self.assertIsNone(host.spawn_zone_drag)

        self.assertFalse(host.begin_select_press(inside, edit_objects=True))
        self.assertEqual(host.spawn_zone_drag.handle, "move")
        host.update_select_drag(QPointF(4.5, 2.5))
        host.end_select_drag(None, None)
        zone = host.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["cols"], zone["rows"]), ([3, 5], [1, 3]))

        self.assertTrue(host.begin_select_press(QPointF(6.5, 6.5)))
        self.assertIsNone(host.selected_spawn_zone_ref)

    def test_object_drag_moves_only_the_chosen_nested_map_end(self) -> None:
        data = empty_map(8, 8)
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [5, 1])]
        host = EditorHost(data, [])

        self.assertTrue(host.begin_select_press(QPointF(5.5, 1.5), edit_objects=True))
        host.end_select_drag((5, 1), (5, 4))
        entry = host.map_data["nested_maps"][0]
        self.assertEqual((entry["from"], entry["to"]), ([1, 1], [5, 4]))
        host.end_select_drag((5, 4), (5, 4))
        self.assertEqual(host.map_data["nested_maps"][0]["to"], [5, 4])
        self.assertTrue(host.begin_select_press(QPointF(3.5, 3.5)))


def nested(map_name: str, level: int, start: list[int], end: list[int], to_level: int | None = None) -> dict:
    return {
        "map": map_name,
        "level": level,
        "from": start,
        "to": end,
        "to_level": level if to_level is None else to_level,
        "travel_secs": 2.0,
        "pause_secs": 1.0,
        "phase_secs": 0.0,
        "from_nudge": [0.0, 0.0, 0.0],
        "to_nudge": [0.0, 0.0, 0.0],
    }


class NestedMapTests(unittest.TestCase):
    def test_a_click_places_a_still_nested_map_and_a_drag_a_sliding_one(self) -> None:
        host = EditorHost(empty_map(8, 8), [])
        host.place_nested_map((1, 1), (1, 1), "cabin", 0, 2.0, 1.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
        host.place_nested_map((4, 1), (6, 3), "cabin", 0, 3.0, 0.5, 2.0, (0.4, 0.0, 0.0), (0.0, 0.0, 0.0))

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
        host.place_nested_map((2, 2), (2, 2), "", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
        self.assertEqual(len(host.map_data["nested_maps"]), 2)
        self.assertTrue(host.statuses[-1].startswith("Nested map not placed"))

    def test_dragging_a_nested_map_end_moves_only_that_end(self) -> None:
        data = empty_map(8, 8)
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [5, 1]), nested("cabin", 0, [2, 5], [2, 5])]
        host = EditorHost(data, [])

        host.drag_nested_map((5, 1), (5, 4))
        self.assertEqual((host.map_data["nested_maps"][0]["from"], host.map_data["nested_maps"][0]["to"]), ([1, 1], [5, 4]))
        host.drag_nested_map((1, 1), (2, 5))
        self.assertTrue(host.statuses[-1].startswith("Nested map end not moved"))
        self.assertEqual(host.map_data["nested_maps"][0]["from"], [1, 1])

    def test_editing_nested_map_properties_can_swap_the_map(self) -> None:
        data = empty_map(8, 8)
        data["levels"].append(upper_level(floor(0, 0)))
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [4, 1])]
        host = EditorHost(data, [])
        key = (0, (1, 1), 0, (4, 1), "cabin")

        host.set_nested_map_properties(key, "loop_a", 1, 3.5, 0.25, 2.0, (0.0, 0.0, 0.0), (0.0, -0.5, 0.5))

        entry = host.map_data["nested_maps"][0]
        self.assertEqual((entry["from"], entry["to"]), ([1, 1], [4, 1]))
        self.assertEqual(
            (entry["map"], entry["to_level"], entry["travel_secs"], entry["pause_secs"], entry["phase_secs"]),
            ("loop_a", 1, 3.5, 0.25, 2.0),
        )
        self.assertEqual((entry["from_nudge"], entry["to_nudge"]), ([0.0, 0.0, 0.0], [0.0, -0.5, 0.5]))

    def test_placing_on_the_same_start_cell_replaces_the_old_nested_map(self) -> None:
        host = EditorHost(empty_map(8, 8), [])
        host.place_nested_map((1, 1), (4, 1), "cabin", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
        host.place_nested_map((1, 1), (1, 4), "loop_a", 0, 2.0, 0.0, 0.0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))

        self.assertEqual([(e["map"], e["to"]) for e in host.map_data["nested_maps"]], [("loop_a", [1, 4])])

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

    def test_nested_maps_round_trip_and_are_the_last_key(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [{**nested("cabin", 0, [2, 2], [4, 2]), "from_nudge": [0.3, 0.0, 0.0], "to_nudge": [0.0, -1.0, 1.01]}]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "nested.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertGreater(text.index('"nested_maps"'), text.index('"ramps"'))
            self.assertEqual(read_map(path)["nested_maps"], data["nested_maps"])

    def test_resize_drops_a_nested_map_with_an_anchor_outside(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [nested("cabin", 0, [0, 0], [5, 0]), nested("cabin", 0, [2, 2], [2, 2])]
        resized = resize_map_data(data, 5, 6, 0, 0)
        self.assertEqual([e["from"] for e in resized["nested_maps"]], [[2, 2]])

    def test_remove_level_drops_spanning_nested_maps_and_renumbers_the_rest(self) -> None:
        data = empty_map(6, 6)
        data["levels"].append(upper_level(floor(0, 0)))
        data["levels"].append(upper_level(floor(0, 0)))
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [1, 1], 1), nested("cabin", 2, [3, 3], [3, 3])]
        after = remove_level_data(data, 1)
        self.assertEqual([(e["level"], e["to_level"]) for e in after["nested_maps"]], [(1, 1)])

    def test_insert_level_keeps_each_nested_end_on_its_storey(self) -> None:
        data = empty_map(6, 6)
        data["levels"].append(upper_level(floor(0, 0)))
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [1, 1], 1)]
        after = insert_level_data(data, 1)
        self.assertEqual((after["nested_maps"][0]["level"], after["nested_maps"][0]["to_level"]), (0, 2))

    def test_erase_nested_maps_clears_only_anchors_touching_the_rectangle(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [4, 1]), nested("cabin", 0, [0, 5], [0, 5])]
        host = EditorHost(data, [])

        host.erase_group_rect(MODE_ERASE_NESTED_MAPS, (4, 0), (5, 1))
        self.assertEqual([e["from"] for e in host.map_data["nested_maps"]], [[0, 5]])
        host.erase_group_rect(MODE_ERASE_NESTED_MAPS, (3, 3), (3, 3))
        self.assertEqual(host.statuses[-1], "Erase Nested Maps: no nested maps in selection.")

    def test_erase_keep_floors_leaves_nested_maps_in_place(self) -> None:
        data = empty_map(6, 6)
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [1, 1])]
        host = EditorHost(data, [])
        host.erase_cell_rect((0, 0), (5, 5), preserve_floors=True)
        self.assertEqual(len(host.map_data["nested_maps"]), 1)
        host.erase_cell_rect((0, 0), (5, 5), preserve_floors=False)
        self.assertEqual(host.map_data["nested_maps"], [])

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

    def test_a_nested_map_spans_its_own_storeys_plus_its_motion(self) -> None:
        from map_editor.normalization import nested_map_spans_level

        lift = nested("cabin", 1, [1, 1], [1, 1], 2)
        self.assertFalse(nested_map_spans_level(lift, 0, 2))
        self.assertTrue(nested_map_spans_level(lift, 1, 2))
        self.assertTrue(nested_map_spans_level(lift, 3, 2))
        self.assertFalse(nested_map_spans_level(lift, 4, 2))

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

    def test_nested_map_nudges_default_to_zero(self) -> None:
        entry = normalize_nested_map({"map": "cabin", "level": 0, "from": [1, 1], "to": [2, 1]})
        self.assertEqual((entry["from_nudge"], entry["to_nudge"]), ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]))
        self.assertEqual(entry["travel_secs"], 2.0)

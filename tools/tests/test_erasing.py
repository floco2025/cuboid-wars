import json
import unittest

from PySide6.QtCore import QPointF

from editor_fixtures import EditorHost, faces, floor, furnished_map, nested
from map_editor.constants import (
    HIT_FLOOR,
    HIT_ITEM,
    HIT_LIGHT,
    HIT_LIGHT_BRIDGE,
    HIT_PRESSURE_PLATE,
    HIT_WALL,
    MODE_ERASE_BARRIERS,
    MODE_ERASE_FLOORS,
    MODE_ERASE_NESTED_MAPS,
    MODE_ERASE_RAMPS,
    MODE_ERASE_SPAWN_ZONES,
    MODE_ERASE_WALLS,
)
from map_editor.erasing import erase_hit
from map_editor.normalization import empty_level, empty_map

KIND = "treasure"
BRIDGE_KIND = "skyway"


def wall(c0: int, r0: int, c1: int, r1: int) -> dict:
    return {"c0": c0, "r0": r0, "c1": c1, "r1": r1, **faces()}


def actor_zone(level: int, c0: int, r0: int, c1: int, r1: int) -> dict:
    return {"level": level, "cols": [c0, c1], "rows": [r0, r1], "kind": "bruiser", "count": 1}


class LayerEraserTests(unittest.TestCase):
    """Each element group's eraser clears only its own element; Erase clears every element."""

    def host(self) -> EditorHost:
        data = empty_map(4, 4)
        data["levels"].append({**empty_level(1), "floors": [floor(0, 0), floor(1, 0)]})
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
        data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "gold"}]
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

    def test_erase_drops_only_what_stood_in_the_rectangle(self) -> None:
        host = self.host()
        level = host.map_data["levels"][0]
        # Records kept invalid for manual repair, away from the erase: the
        # cell (2, 2) has no floor and its north side no wall.
        level["grass"] = [{"col": 0, "row": 0}, {"col": 2, "row": 2}]
        level["lights"] = [
            {"col": 0, "row": 0, "side": "N", "kind": "utility"},
            {"col": 2, "row": 2, "side": "N", "kind": "utility"},
        ]
        host.map_data["items"].append({"level": 0, "col": 2, "row": 2, "type": "gold"})
        host.erase_group_rect(MODE_ERASE_FLOORS, (0, 0), (1, 1))
        level = host.map_data["levels"][0]
        self.assertEqual(level["grass"], [{"col": 2, "row": 2}])
        self.assertEqual(host.map_data["items"], [{"level": 0, "col": 2, "row": 2, "type": "gold"}])
        self.assertEqual(len(level["lights"]), 2)
        host.erase_group_rect(MODE_ERASE_WALLS, (0, 0), (1, 1))
        level = host.map_data["levels"][0]
        self.assertEqual(level["lights"], [{"col": 2, "row": 2, "side": "N", "kind": "utility"}])

    def test_erasing_one_wall_takes_only_its_own_lights(self) -> None:
        data = self.host().map_data
        data["levels"][0]["lights"] = [
            {"col": 0, "row": 0, "side": "N", "kind": "utility"},
            {"col": 0, "row": 0, "side": "S", "kind": "utility"},
            {"col": 3, "row": 3, "side": "N", "kind": "utility"},
        ]
        after = erase_hit(data, 0, (HIT_WALL, (0, 0, 1, 0)))
        self.assertEqual(
            after["levels"][0]["lights"],
            [{"col": 0, "row": 0, "side": "S", "kind": "utility"}, {"col": 3, "row": 3, "side": "N", "kind": "utility"}],
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
        self.assertEqual(host.hit_at(bridge_center), (HIT_LIGHT_BRIDGE, (1, 0)))
        host.erase_at(bridge_center, preserve_floors=True)
        self.assertEqual(host.map_data["levels"][0]["light_bridges"], [{"col": 1, "row": 0, "kind": BRIDGE_KIND}])

        host.erase_at(bridge_center, preserve_floors=False)
        self.assertEqual(host.map_data["levels"][0]["light_bridges"], [])

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


class RightClickTests(unittest.TestCase):
    def test_right_click_peels_the_light_then_plate_item_and_floor_off_a_cell(self) -> None:
        host = EditorHost(furnished_map(), ["bridge_1"])
        near_top = QPointF(1.5, 1.05)
        center = QPointF(1.5, 1.5)

        self.assertEqual(host.hit_at(near_top), (HIT_LIGHT, (1, 1, "N")))
        host.erase_hit((HIT_LIGHT, (1, 1, "N")))
        self.assertEqual(host.map_data["levels"][0]["lights"], [])
        self.assertEqual(host.hit_at(near_top)[0], HIT_WALL)

        self.assertEqual(host.hit_at(center), (HIT_PRESSURE_PLATE, (1, 1)))
        self.assertEqual([p["type"] for p in host.plates_at(1, 1)], ["barrier"])
        host.erase_hit((HIT_PRESSURE_PLATE, (1, 1)))
        self.assertEqual(host.map_data["pressure_plates"], [])

        self.assertEqual(host.hit_at(center), (HIT_ITEM, (1, 1)))
        host.erase_hit((HIT_ITEM, (1, 1)))
        self.assertEqual(host.map_data["items"], [])
        self.assertEqual(host.hit_at(center), (HIT_FLOOR, (1, 1)))

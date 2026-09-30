import unittest
from dataclasses import replace
from math import pi

from editor_fixtures import floor
from jump_fixtures import jump_settings, scenario
from map_editor.floor_footprints import FloorFootprints
from map_editor.jump_path import (
    AFTER_EXIT,
    BEFORE_ENTRY,
    Flight,
    Takeoff,
    build_request,
    entry_outcome,
    jump_preview,
    level_regions,
    level_view,
    outline_segments,
    takeoff_from_click,
)
from map_editor.jump_settings import ANTI_GRAVITY, BOTH, NORMAL, SCENARIO_BITS, SPEED
from map_editor.normalization import empty_level, empty_map
from map_editor.portal_surfaces import PortalSurface, PortalSurfaces, portals_overlap


def two_storeys(cells=()):
    data = empty_map(8, 8)
    data["levels"].append(empty_level(1))
    data["levels"][1]["floors"] = [floor(col, row) for col, row in cells]
    return data


class TakeoffTests(unittest.TestCase):
    def setUp(self):
        self.settings = jump_settings()
        self.data = two_storeys([(2, 2), (3, 2)])
        self.footprints = FloorFootprints(self.data, self.settings.cell_size, self.settings.wall_thickness)

    def click(self, x, z, level=1):
        return takeoff_from_click(level, x, z, 8, 8, self.footprints.cells)

    def test_a_click_takes_off_over_the_nearest_edge_at_the_point_clicked(self):
        for (x, z), side, along, heading, on_line in (
            ((2.3, 2.1), "N", 0.3, (0, -1), (2.3, 2)),
            ((2.7, 2.9), "S", 0.7, (0, 1), (2.7, 3)),
            ((2.1, 2.6), "W", 0.6, (-1, 0), (2, 2.6)),
            ((3.9, 2.4), "E", 0.4, (1, 0), (4, 2.4)),
        ):
            with self.subTest(side=side):
                takeoff = self.click(x, z)
                self.assertEqual((takeoff.level, takeoff.side), (1, side))
                self.assertAlmostEqual(takeoff.along, along)
                self.assertEqual(takeoff.direction, heading)
                for actual, expected in zip(takeoff.grid_point, on_line):
                    self.assertAlmostEqual(actual, expected)
        self.assertIsNone(self.click(8.2, 2.5))
        self.assertIsNone(self.click(2.5, -0.1))

    def test_a_click_just_past_a_ledge_takes_off_from_the_slab(self):
        takeoff = self.click(4.1, 2.25)
        self.assertEqual((takeoff.col, takeoff.row, takeoff.side), (3, 2, "E"))
        self.assertAlmostEqual(takeoff.along, 0.25)
        above = self.click(2.5, 1.95)
        self.assertEqual(replace(above, along=0), Takeoff(1, 2, 2, "N", 0))
        # An empty cell away from any slab is a planned floor of its own.
        planned = self.click(6.1, 6.5)
        self.assertEqual((planned.col, planned.row, planned.side), (6, 6, "W"))

    def test_the_takeoff_point_is_on_the_slabs_real_edge(self):
        exposed = Takeoff(1, 3, 2, "E", 0.5).point(self.settings, self.footprints)
        self.assertEqual(exposed, (16.2, 5, 10.0))
        joined = Takeoff(1, 2, 2, "E", 0.5).point(self.settings, self.footprints)
        self.assertEqual(joined, (12, 5, 10.0))
        north = Takeoff(1, 2, 2, "N", 0.25).point(self.settings, self.footprints)
        self.assertEqual(north, (9.0, 5, 7.8))

    def test_the_request_carries_metres_and_drops_the_margin_for_a_step(self):
        takeoff = Takeoff(1, 3, 2, "E", 0.5)
        request = build_request(
            self.settings,
            self.footprints,
            takeoff,
            levels=2,
            jumping=True,
            margin=0.1,
            air_control=True,
            shooter=takeoff.grid_point,
        )
        self.assertEqual(
            request,
            {
                "takeoff": {"point": [16.2, 5, 10.0], "direction": [1.0, 0.0], "jumping": True, "margin": 0.1},
                "heights": [0, 5],
                "air_control": True,
                "shooter": [16.0, 10.0],
                "portals": None,
            },
        )
        entry = PortalSurface.floor_at(0, 5.25, 2.5).placed_from(takeoff.grid_point)
        request = build_request(
            self.settings,
            self.footprints,
            takeoff,
            levels=2,
            jumping=False,
            margin=0.1,
            air_control=False,
            shooter=(1.0, 1.0),
            entry=entry,
            exit=PortalSurface(0, 6, 4, "west"),
        )
        self.assertEqual(request["takeoff"]["margin"], 0.0)
        self.assertEqual(request["shooter"], [4.0, 4.0])
        self.assertEqual(request["portals"]["entry"], {"center": [21.0, 0, 10.0], "normal": [0, 1, 0], "yaw": pi / 2})
        self.assertEqual(request["portals"]["exit"]["normal"], [-1, 0, 0])

    def test_a_jump_is_flown_for_every_scenario_in_map_cores_order(self):
        takeoff = Takeoff(1, 3, 2, "E", 0.5)
        request = build_request(
            self.settings,
            self.footprints,
            takeoff,
            levels=2,
            jumping=True,
            margin=0.0,
            air_control=False,
            shooter=takeoff.grid_point,
        )
        flights = jump_preview(self.settings, request)
        self.assertEqual(tuple(flight.bit for flight in flights), SCENARIO_BITS)
        # Speed 1 and jump 2 under gravity 2 come back down on the takeoff level 2 m out; the pickup doubles it.
        reach = {flight.bit: flight.crossings[0].point[0] - 16.2 for flight in flights}
        self.assertAlmostEqual(reach[NORMAL], 2, places=2)
        self.assertAlmostEqual(reach[SPEED], 4, places=2)
        self.assertAlmostEqual(reach[ANTI_GRAVITY], 4, places=2)
        self.assertAlmostEqual(reach[BOTH], 8, places=2)
        self.assertTrue(all(flight.crossings[0].level == 1 and flight.end == "below" for flight in flights))
        self.assertTrue(all(flight.capture[0] and flight.capture[1] for flight in flights))


class PortalSurfaceTests(unittest.TestCase):
    def setUp(self):
        self.settings = jump_settings(wall_thickness=0.2)
        self.data = empty_map(30, 30)
        self.data["levels"] = [empty_level(i) for i in range(3)]

    def test_a_wall_pick_takes_the_nearest_edge_and_a_floor_pick_the_point_itself(self):
        surfaces = PortalSurfaces(self.data, {})
        for x, z, side in ((5.5, 4.95, "north"), (5.5, 5.05, "south"), (4.95, 5.5, "west"), (5.05, 5.5, "east")):
            self.assertEqual(surfaces.pick_wall(0, x, z).face, side)
        far = surfaces.pick_wall(0, 5.4, 5.3)
        self.assertEqual((far.face, far.col, far.row), ("south", 5, 5))
        edge = surfaces.pick_wall(0, 30.1, 4.5)
        self.assertEqual((edge.face, edge.col, edge.row), ("east", 30, 4))
        self.assertIsNone(surfaces.pick_wall(0, 33, 33))
        picked = surfaces.pick_floor(0, 5.3, 5.8)
        self.assertEqual((picked.face, picked.col, picked.row), ("floor", 5, 5))
        self.assertAlmostEqual(picked.offset[0], 0.3)
        self.assertAlmostEqual(picked.offset[1], 0.8)
        self.assertEqual(picked.frame(self.settings).center, (5.3 * 4, 0, 5.8 * 4))
        corner = surfaces.pick_floor(0, 6.0, 7.0)
        self.assertEqual((corner.face, corner.grid_center), ("floor", (6.0, 7.0)))
        self.assertIsNone(surfaces.pick_floor(0, 31, 31))

    def test_a_floor_portal_lies_along_the_quarter_turn_its_shooter_faces(self):
        surface = PortalSurface.floor_at(0, 5.5, 5.5)
        for shooter, up in (
            ((5.5, 2.0), (0, 0, 1)),
            ((5.5, 9.0), (0, 0, -1)),
            ((2.0, 5.5), (1, 0, 0)),
            ((9.0, 5.2), (-1, 0, 0)),
            ((4.0, 3.5), (0, 0, 1)),
        ):
            with self.subTest(shooter=shooter):
                self.assertEqual(surface.placed_from(shooter).frame(self.settings).up, up)
        wall = PortalSurface(0, 5, 5, "north")
        self.assertIs(wall.placed_from((0.0, 0.0)), wall)

    def test_surface_materials_ramps_terrain_and_planned_surfaces(self):
        target = PortalSurface.floor_at(0, 5.2, 5.7)
        self.data["levels"][0]["floors"] = [dict(floor(5, 5), top="resistant")]
        surfaces = PortalSurfaces(self.data, {"basement-floor": True, "resistant": False})
        self.assertFalse(surfaces.status(target).available)
        self.assertFalse(surfaces.status(target).planned)
        self.assertTrue(surfaces.status(PortalSurface.floor_at(0, 6.5, 5.5)).planned)
        self.data["levels"][0]["floors"][0]["top"] = "basement-floor"
        surfaces = PortalSurfaces(self.data, {"basement-floor": True})
        self.assertTrue(surfaces.status(target).available)
        self.assertFalse(surfaces.status(target).planned)
        self.data["levels"][0]["terrain"] = [{"col": 9, "row": 9}]
        self.data["ramps"] = [{"lower_level": 0, "cols": [5, 6], "rows": [5, 8], "direction": "S"}]
        surfaces = PortalSurfaces(self.data, {"basement-floor": True})
        self.assertIn("Ramp", surfaces.status(target).reason)
        self.assertIn("Terrain", surfaces.status(PortalSurface.floor_at(0, 9.5, 9.5)).reason)

    def test_wall_faces_keep_their_own_permission_and_rest_their_rim_on_the_base(self):
        settings = jump_settings(level_height=2.4, wall_thickness=0.2)
        for level in (0, 1):
            self.data["levels"][level]["walls"] = [
                {"c0": 5, "r0": 5, "c1": 6, "r1": 5, "north": "allowed", "south": "blocked"}
            ]
        surfaces = PortalSurfaces(self.data, {"allowed": True, "blocked": False})
        for level in (0, 1, 2):
            north = PortalSurface(level, 5, 5, "north")
            self.assertTrue(surfaces.status(north).available)
            self.assertEqual(surfaces.status(north).planned, level == 2)
            self.assertAlmostEqual(north.frame(settings).center[1] - 1.378, level * 2.4)
        self.assertFalse(surfaces.status(PortalSurface(0, 5, 5, "south")).available)
        self.assertEqual(PortalSurface(0, 5, 5, "north").frame(settings).center[2], 5 * 4 - 0.1)

    def test_portals_overlap_where_their_apertures_meet(self):
        a = PortalSurface.floor_at(0, 5.5, 5.5)
        self.assertTrue(portals_overlap(a, replace(a, turn=1), self.settings))
        self.assertTrue(portals_overlap(a, PortalSurface.floor_at(0, 5.8, 5.5), self.settings))
        self.assertFalse(portals_overlap(a, PortalSurface.floor_at(0, 6.0, 5.5), self.settings))
        self.assertFalse(portals_overlap(a, PortalSurface.floor_at(1, 5.5, 5.5), self.settings))
        wide = jump_settings(portal_size={"width": 4.4, "height": 3.8})
        self.assertTrue(portals_overlap(a, PortalSurface.floor_at(0, 6.0, 5.5), wide))
        self.assertAlmostEqual(PortalSurface(0, 1, 1, "east").frame(wide).center[1], 1.9 * 1.06)


class LevelViewTests(unittest.TestCase):
    def setUp(self):
        self.settings = jump_settings()
        # Level 1 has a floor at (4, 2); level 0 has none.
        self.footprints = FloorFootprints(two_storeys([(4, 2)]), 4, 0.4)

    def flight(self, *args, **fields):
        return Flight.parse(NORMAL, scenario(*args, **fields))

    def view(self, flight, level):
        return level_view(flight, level, self.settings, self.footprints)

    def test_a_path_is_inside_until_it_falls_past_the_viewed_floor(self):
        # The arc rises above the storey overhead, 5 m up, and is still this level's jump.
        flight = self.flight([(16, 5, 10), (17, 11, 10), (18, 6, 10), (19, 4, 10), (20, 1, 10), (21, -3, 10)])
        upper = self.view(flight, 1).runs
        self.assertEqual([run.inside for run in upper], [True, False])
        self.assertEqual(upper[0].points[:3], ((16, 10), (17, 10), (18, 10)))
        # The split is where the path meets the floor, inside the tick.
        self.assertAlmostEqual(upper[0].points[3][0], 18.5, places=2)
        self.assertEqual(upper[1].points[0], upper[0].points[3])
        self.assertEqual(upper[1].points[1:], ((19, 10), (20, 10), (21, 10)))
        lower = self.view(flight, 0).runs
        self.assertEqual([run.inside for run in lower], [True, False])
        self.assertEqual(len(lower[0].points), 6)
        self.assertAlmostEqual(lower[0].points[-1][0], 20.25, places=2)
        self.assertTrue(all(run.phase == BEFORE_ENTRY for run in upper + lower))
        # A step leaves the floor at once: nothing of it is inside its own level.
        step = self.flight([(16, 5, 10), (17, 4.9, 10), (18, 4.6, 10)])
        self.assertEqual([run.inside for run in self.view(step, 1).runs], [False])
        # Storey heights that are not whole numbers still hold the point that stands on them.
        tall = level_view(self.flight([(0, 13.2, 0), (1, 14, 0)]), 3, jump_settings(level_height=4.4), self.footprints)
        self.assertEqual([run.inside for run in tall.runs], [True])

    def test_a_hop_is_not_travelled_and_starts_the_exit_phase(self):
        path = [(16, 5, 10), (17, 5.5, 10), (18, 5.1, 10), (2, 5.2, 30), (3, 5.4, 30)]
        runs = self.view(self.flight(path, hop=2, hop_time=0.07), 1).runs
        self.assertEqual(
            [(run.phase, run.points) for run in runs],
            [
                (BEFORE_ENTRY, ((16, 10), (17, 10), (18, 10))),
                (AFTER_EXIT, ((2, 30), (3, 30))),
            ],
        )

    def test_glyphs_know_their_floor_and_the_landing_that_stopped_the_flight_before_them(self):
        path = [(16, 5, 10), (21, -1, 10)]
        caught = self.flight(path, [(1, BEFORE_ENTRY, (18, 10), 1.0, 0.0), (0, BEFORE_ENTRY, (20, 10), 2.0, 0.5)])
        (upper,) = self.view(caught, 1).glyphs
        self.assertTrue(upper.supported and not upper.blocked)
        (lower,) = self.view(caught, 0).glyphs
        self.assertEqual((lower.point, lower.damage), ((20, 10), 0.5))
        self.assertTrue(lower.blocked and not lower.supported)
        clear = self.flight(path, [(1, BEFORE_ENTRY, (24, 10), 1.0, 0.0), (0, BEFORE_ENTRY, (26, 10), 2.0, 1.0)])
        self.assertFalse(self.view(clear, 1).glyphs[0].supported)
        self.assertFalse(self.view(clear, 0).glyphs[0].blocked)

    def test_regions_are_collected_for_the_viewed_level(self):
        square = [[0, 0], [1, 0], [1, 1], [0, 1]]
        flight = self.flight(
            [(0, 5, 0), (1, 5, 0)],
            capture=[{"level": 0, "pieces": [{"yaw": 0, "polygon": square}, {"yaw": 1.571, "polygon": square}]}],
            capture_steered=[{"level": 1, "pieces": [{"yaw": 0, "polygon": square}]}],
            range=[{"level": 0, "polygon": square[:2]}],
        )
        lower = level_regions(flight, 0, (50, 50))
        self.assertEqual([region.kind for region in lower], ["capture", "range"])
        self.assertEqual(len(lower[0].segments), 8)
        self.assertEqual(lower[0].segments[0], ((0, 0), (1, 0)))
        self.assertEqual(lower[1].segments, (((0, 0), (1, 0)),))
        self.assertEqual([region.kind for region in level_regions(flight, 1, (50, 50))], ["capture_steered"])
        self.assertEqual(level_regions(flight, 2, (50, 50)), [])

    def test_a_run_is_one_stroke_and_a_point_is_none(self):
        self.assertEqual(outline_segments([((0, 0), (1, 0))], (50, 50)), [((0, 0), (1, 0))])
        self.assertEqual(outline_segments([((3, 4),)], (50, 50)), [])

    def test_pieces_cut_along_a_diagonal_through_the_shooter_outline_as_one_region(self):
        # Two quarter-turn pieces meet on the diagonal from the shooter at (1, 1):
        # one covers it from 2 to 5, the other from 3 to 6.
        east = ((2, 2), (5, 5), (5, 1), (2, 1))
        south = ((3, 3), (3, 7), (6, 7), (6, 6))
        segments = outline_segments([east, south], (1, 1))
        diagonal = sorted(
            segment for segment in segments if segment[0][0] - segment[0][1] == 0 == segment[1][0] - segment[1][1]
        )
        self.assertEqual(diagonal, [((2, 2), (3, 3)), ((5, 5), (6, 6))])
        self.assertEqual(len(segments), 3 + 3 + 2)
        # Shot from elsewhere, the same edges are ordinary outline.
        self.assertEqual(len(outline_segments([east, south], (0, 3))), 8)

    def test_an_entry_is_blocked_by_a_floor_the_flight_meets_first(self):
        path = [(16, 5, 10), (18, 5, 10), (20, 0, 10)]
        for crossings, fields, outcome in (
            ([(1, BEFORE_ENTRY, (24, 10), 1.0, 0.0)], dict(hop=2, hop_time=2.0, entry="funnel"), "funnel"),
            ([(1, BEFORE_ENTRY, (18, 10), 1.0, 0.0)], dict(hop=2, hop_time=2.0, entry="direct"), "blocked"),
            ([(1, AFTER_EXIT, (18, 10), 3.0, 0.0)], dict(hop=2, hop_time=2.0, entry="steered"), "steered"),
            ([(1, BEFORE_ENTRY, (18, 10), 1.0, 0.0)], dict(entry="missed"), "missed"),
            ([], {}, None),
        ):
            with self.subTest(outcome=outcome):
                self.assertEqual(entry_outcome(self.flight(path, crossings, **fields), self.footprints), outcome)


if __name__ == "__main__":
    unittest.main()

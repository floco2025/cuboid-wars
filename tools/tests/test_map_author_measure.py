from unittest.mock import patch

from author_fixtures import course
from config_fixtures import ConfigTestCase
from jump_fixtures import scenario
from map_author import measure
from map_author.measure import jump, parse_surface, parse_takeoff, ranges, surface, surface_spec, where
from map_editor.jump_path import Flight, Preview, Takeoff
from map_editor.jump_settings import SCENARIO_BITS
from map_editor.portal_surfaces import PortalSurface


class SpecTests(ConfigTestCase):
    def test_takeoff_specs(self):
        self.assertEqual(parse_takeoff("L4:11,17:E"), Takeoff(4, 11, 17, "E", 0.5))
        self.assertEqual(parse_takeoff("L0:3,2:N:0.25"), Takeoff(0, 3, 2, "N", 0.25))
        with self.assertRaises(ValueError):
            parse_takeoff("L4:11,17")

    def test_wall_specs_name_the_cell_a_portal_faces(self):
        cases = {
            "wall:L1:3,3:S": PortalSurface(1, 3, 4, "north"),
            "wall:L1:4,3:N": PortalSurface(1, 4, 3, "south"),
            "wall:L1:4,3:W": PortalSurface(1, 4, 3, "east"),
            "wall:L1:4,3:E": PortalSurface(1, 5, 3, "west"),
        }
        for spec, expected in cases.items():
            with self.subTest(spec=spec):
                self.assertEqual(parse_surface(spec), expected)
                self.assertEqual(surface_spec(expected), spec)

    def test_a_wall_spec_may_say_how_far_along_its_edge(self):
        shared = parse_surface("wall:L1:6,2:N:1")
        self.assertEqual((shared.grid_center, surface_spec(shared)), ((7.0, 2), "wall:L1:6,2:N:1"))
        self.assertEqual(parse_surface("wall:L1:6,2:W:0.25").grid_center, (6, 2.25))
        self.assertEqual(parse_surface("wall:L1:6,2:N").grid_center, (6.5, 2))

    def test_floor_specs_are_points_in_cells(self):
        self.assertEqual(parse_surface("floor:L0:3,6"), PortalSurface(0, 3, 6, offset=(0.0, 0.0)))
        self.assertEqual(surface_spec(PortalSurface(0, 3, 6, offset=(0.5, 0.25))), "floor:L0:3.5,6.25")
        with self.assertRaises(ValueError):
            parse_surface("ceiling:L0:3,6")


class ViewTests(ConfigTestCase):
    def test_where_converts_both_ways(self):
        ctx = course()
        self.assertEqual(where(ctx, "L1:4,3"), "L1 cell (4, 3) is world (-6.80, 2.40, -3.40)")
        self.assertEqual(where(ctx, world="-6.8,2.4,-3.4"), "world (-6.8, 2.4, -3.4) is L1 cell (4.00, 3.00)")
        self.assertEqual(where(ctx, world="0,3.5,0"), "world (0, 3.5, 0) is between L1/L2 (y=3.50) cell (6.00, 4.00)")

    def test_surface_reports_readiness(self):
        ctx = course()
        self.assertIn("ready: 2 portalable storeys with a clear front", surface(ctx, parse_surface("wall:L1:3,3:S")))
        self.assertIn("not ready: no wall section on the storey above", surface(ctx, parse_surface("wall:L1:1,2:N")))
        self.assertIn("ready: pad cols 2..4 rows 5..7", surface(ctx, parse_surface("floor:L0:3,6")))
        self.assertIn("not ready: a pressure plate", surface(ctx, parse_surface("floor:L0:7,6")))
        self.assertIn("no wall on that edge", surface(ctx, parse_surface("wall:L1:9,3:E")))


def canned(flights):
    def fake(settings, request):
        return Preview(tuple(Flight.parse(bit, flight) for bit, flight in zip(SCENARIO_BITS, flights)), 0.21)

    return fake


class FlightTests(ConfigTestCase):
    def test_jump_lists_each_crossing_until_the_landing(self):
        ctx = course()
        # East off the west platform, in grid-origin metres: the east platform starts at x = 7 * 3.4.
        flight = scenario(
            [[13.6, 2.4, 10.2]],
            crossings=[
                (2, "before_entry", (15.0, 10.2), 0.3, 0.0),
                (1, "before_entry", (24.5, 10.2), 0.9, 0.0),
                (0, "before_entry", (26.0, 10.2), 1.3, 0.1),
            ],
        )
        with patch.object(measure, "jump_preview", canned([flight] * 4)):
            text = jump(ctx, parse_takeoff("L1:3,3:E"))
        lines = text.splitlines()
        self.assertEqual(
            lines[0], "takeoff L1 cell (3, 3) side E along 0.5: jump heading E, world (-6.65, 2.40, -1.70)"
        )
        self.assertEqual(lines[1], "normal: falls below every floor")
        self.assertEqual(
            lines[2], "  L2 at 0.30 s: cell (4.4, 3.0) world (-5.40, 4.80, -3.40)  0% damage  no floor there"
        )
        self.assertEqual(
            lines[3], "  L1 at 0.90 s: cell (7.2, 3.0) world (4.10, 2.40, -3.40)  0% damage  LANDS on cell (7, 2)"
        )
        self.assertEqual(lines[4], "speed: falls below every floor")

    def test_a_fatal_crossing_ends_the_listing(self):
        ctx = course()
        flight = scenario(
            [[13.6, 2.4, 10.2]],
            crossings=[(0, "before_entry", (40.0, 10.2), 2.0, 1.0)],
        )
        with patch.object(measure, "jump_preview", canned([flight] * 4)):
            lines = jump(ctx, parse_takeoff("L1:3,3:E"), walk=True).splitlines()
        self.assertTrue(lines[0].endswith("walk-off heading E, world (-6.65, 2.40, -1.70)"))
        self.assertEqual(lines[2], "  L0 at 2.00 s: cell (11.8, 3.0) world (19.60, 0.00, -3.40)  fatal  no floor there")
        self.assertEqual(len(lines), 9)

    def test_fling_reports_the_entry_and_refuses_bad_surfaces(self):
        ctx = course()
        flight = scenario(
            [[13.6, 2.4, 10.2], [14.0, 2.4, 10.2], [15.0, 2.4, 10.2]],
            hop=0,
            hop_time=0.03,
            end="below",
            entry="direct",
            crossings=[(1, "after_exit", (15.0, 10.2), 0.2, 0.0)],
        )
        with patch.object(measure, "jump_preview", canned([flight] * 4)):
            text = jump(ctx, parse_takeoff("L1:3,3:S"), entry=parse_surface("wall:L1:3,3:S"), into=True)
        self.assertIn("walk into the wall heading S", text)
        self.assertIn("portal 1 wall:L1:3,3:S, no portal 2", text)
        self.assertIn("normal: falls below every floor, portal 1 direct at 0.03 s", text)
        self.assertIn("  L1 after exit at 0.20 s:", text)
        with self.assertRaisesRegex(ValueError, "does not allow"):
            jump(ctx, parse_takeoff("L1:7,2:N"), entry=parse_surface("wall:L1:7,2:N"))
        with self.assertRaisesRegex(ValueError, "walk into"):
            jump(ctx, parse_takeoff("L1:1,2:N"), entry=parse_surface("wall:L1:3,3:S"), into=True)

    def test_held_input_reach_comes_from_the_range_hull(self):
        ctx = course()
        flight = scenario(
            [[13.6, 2.4, 10.2]],
            crossings=[(0, "before_entry", (20.0, 10.2), 1.0, 0.0)],
            range=[{"level": 0, "polygon": [[14.0, 8.0], [22.0, 8.0], [22.0, 13.0], [14.0, 13.0]]}],
        )
        with patch.object(measure, "jump_preview", canned([flight] * 4)):
            text = jump(ctx, parse_takeoff("L1:3,3:E"), air_control=True)
        self.assertIn(
            "  holding a direction: on L0 reaches up to 8.2 m (2.4 cells) along the heading and 3.9 m to either side",
            text,
        )

    def test_ranges_run_the_real_physics(self):
        lines = ranges(course()).splitlines()
        held = next(line for line in lines if line.startswith("jump, held      normal"))
        released = next(line for line in lines if line.startswith("jump            normal"))
        level_reach = float(held.split()[-4])
        self.assertGreater(level_reach, float(released.split()[-4]))
        self.assertTrue(5 < level_reach < 14, level_reach)
        self.assertTrue(lines[-1].startswith("apex 2.88 m normal"))

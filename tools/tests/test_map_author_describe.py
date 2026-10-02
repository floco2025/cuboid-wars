from author_fixtures import course
from config_fixtures import ConfigTestCase
from map_author.describe import plan, summary
from map_author.index import MapIndex


class PlanTests(ConfigTestCase):
    def test_the_plan_crops_to_content_and_draws_every_record(self):
        text = plan(course(), 1, legend=False)
        lines = text.splitlines()
        self.assertEqual(
            lines,
            [
                '## L1 "Deck"  y=4.40  cols 1..8  rows 2..3',
                "     1 2 3 4 5 6 7 8",
                "     ~ ~ ~       - -",
                "  2  0 . .%_ _ _x. .",
                "  3  . s .      x. .",
                "         =",
            ],
        )

    def test_the_lower_level_draws_the_ramp_ladder_pads_and_plate(self):
        lines = plan(course(), 0, legend=False).splitlines()
        self.assertEqual(lines[0], '## L0 "Level 0"  y=0.00  cols 1..10  rows 0..6')
        self.assertEqual(lines[3], "  0                  > >")
        self.assertEqual(lines[6], "     H")
        self.assertEqual(lines[-2:], ["  5    O O     @ o", "  6    O O     o o"])

    def test_an_empty_level_says_so_and_the_legend_prints_once(self):
        ctx = course()
        ctx.data["levels"][2]["walls"] = []
        text = plan(ctx)
        self.assertIn('## L2 "Level 2"  y=8.80  (empty)', text)
        self.assertEqual(text.count("legend:"), 1)


class SummaryTests(ConfigTestCase):
    def test_platforms_gaps_and_holdings(self):
        lines = summary(course()).splitlines()
        self.assertEqual(lines[0], "map obby  12x8 cells  cell 3.4 m  level 4.4 m  world x -20.4..20.4  z -13.6..13.6")
        self.assertIn(
            "  L1.a   cols 1..4 rows 2..4 (3x2) floor  world x -17..-6.8 z -6.8..0  holds: cp0, speed (2, 3)", lines
        )
        self.assertIn("  L1.b   cols 7..9 rows 2..4 (2x2) floor  world x 3.4..10.2 z -6.8..0", lines)
        self.assertIn("  bridge 'door' cols 4..7 rows 2..3 (3x1)", lines)
        self.assertIn("  gaps: L1.a -E-> L1.b 3 cells (9.9 m edge to edge) dy +0 (+0.0 m)", lines)

    def test_portal_readiness_verdicts(self):
        ctx = course()
        index = MapIndex(ctx)
        pads = {min(s.cells): (s.ready, s.reason) for s in index.floor_surfaces(0)}
        self.assertTrue(pads[(2, 5)][0])
        self.assertEqual(pads[(6, 5)], (False, "a pressure plate on the pad blocks portals within 1.2 m"))
        walls = {(s.key, s.face): s.reason for s in index.wall_surfaces(1)}
        self.assertEqual(walls[("h", 3, 4), "north"], "")
        self.assertIn("no wall section on the storey above", walls[("h", 1, 2), "south"])
        text = summary(ctx)
        self.assertIn("wall  wall:L1:3,3:S  faces north", text)
        self.assertIn("floor floor:L0:3,6  cols 2..4 rows 5..7  long axis x and z", text)
        self.assertIn("portalable but not ready:", text)

    def test_structure_lines(self):
        text = summary(course())
        self.assertIn("ramp L0->L1 cols 9..11 rows 0..1 rising E (solid)  32.9° climbable", text)
        self.assertIn("ladder L0->L1 on side S of cell (1, 2), climbed from the cell across that edge", text)
        self.assertIn("barrier 'door' L1 between cells (3, 2) and (4, 2)", text)
        self.assertIn("eraser L1 between cells (6, 2)..(6, 3) and (7, 2)..(7, 3) (2 edges)", text)
        self.assertIn("switch 's': toggle, reset solo, held any; plates: L0 (6, 5)", text)
        self.assertIn("field 'door': initially off, switch 's'; 1 barrier edges, 3 bridge cells", text)
        self.assertIn("fireworks on switch 's'", text)

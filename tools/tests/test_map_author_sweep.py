from author_fixtures import hall
from config_fixtures import ConfigTestCase
from map_author.sweep import Goal, parse_moves, sweep

MOVES = "move 0,-1 x40 jump; advance 20"
NORTH = [0.0, 0.0, 1.0]


def player(position, support="ground"):
    return {"position": position, "support": support}


# A game in which every wall and floor target opens a portal, and an attempt
# ends where `ends` says for the portals it placed.
def game(ends):
    calls = []

    def run(ctx, spawn, actions):
        calls.append(actions)
        if actions[0]["action"] == "probe":
            results = []
            for action in actions:
                shots = []
                for target in action["targets"]:
                    hit = {"position": target, "normal": NORTH if target[1] > 3 else [0.0, 1.0, 0.0]}
                    opens = target[1] > 2.1
                    portal = {"position": target, "normal": NORTH, "yaw": 0.0}
                    shots.append(
                        {"target": target, "status": "placed", "hit": hit, "portal": portal}
                        if opens
                        else {"target": target, "status": "no_surface"}
                    )
                results.append({"result": {"status": "probed", "shots": shots}})
            return {"steps": results}
        steps, placed = [], []
        for action in actions:
            if action["action"] == "reset":
                placed = []
            if action["action"] == "place":
                placed.append(tuple(action["target"]))
            crossings, state = ends(tuple(placed))
            events = [{"kind": "player_portal_crossing"}] * crossings if action["action"] == "inspect" else []
            steps.append(
                {"action": action, "result": {"status": "submitted"}, "events": events, "state": {"player": state}}
            )
        return {"steps": steps}

    run.calls = calls
    return run


class ParseTests(ConfigTestCase):
    def test_moves_are_the_runners_actions(self):
        self.assertEqual(
            parse_moves(MOVES),
            [
                {"action": "move", "direction": [0.0, -1.0], "ticks": 40, "jump": True, "crouch": False},
                {"action": "advance", "ticks": 20},
            ],
        )
        for text in ("", "walk north", "move 1 x4"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_moves(text)

    def test_a_goal_is_a_rectangle_of_cells_on_a_level(self):
        ctx = hall()
        goal = Goal.parse("L1:2,2:6,4")
        self.assertTrue(goal.holds(ctx, player([-6.0, 2.2, -7.0])))
        self.assertFalse(goal.holds(ctx, player([-6.0, 2.2, -7.0], "air")))
        self.assertFalse(goal.holds(ctx, player([-6.0, 4.4, -7.0])))
        self.assertFalse(goal.holds(ctx, None))
        with self.assertRaises(ValueError):
            Goal.parse("L1:2,2")


class SweepTests(ConfigTestCase):
    def test_every_pair_is_tried_once_from_a_reset(self):
        ctx = hall()
        start = player([-6.0, 2.2, -4.0])

        def ends(placed):
            if len(placed) == 2 and placed[1][1] == 2.2:
                return 2, None
            if len(placed) == 2:
                return 1, player([-5.5, 2.2, -7.2])
            return 0, start

        run = game(ends)
        lines = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:6,4", run=run).splitlines()
        self.assertEqual(lines[0], f"sweep from L1 (4, 6): 3 portals in reach, 3 pairs, moves: {MOVES}")
        self.assertEqual(lines[1], "no portals: ends L1 cell (4.0, 6.0) on L1.a")
        self.assertEqual(
            lines[2], "wall:L1:5,2:N:1 + wall:L1:7,2:N:1: crosses 1, ends L1 cell (4.5, 2.8) on L1.a  GOAL"
        )
        self.assertEqual(lines[3], "wall:L1:5,2:N:1 + floor:L1:9.5,6.5: crosses 2, DIES")
        self.assertEqual(lines[-2], "3 of 3 pairs carry the body through a portal")
        self.assertEqual(lines[-1], "1 reach the goal: wall:L1:5,2:N:1 + wall:L1:7,2:N:1")
        probe, attempts = run.calls
        self.assertEqual(len(probe), 1)
        kinds = [action["action"] for action in attempts]
        self.assertEqual(kinds[:4], ["reset", "move", "advance", "inspect"])
        self.assertEqual(kinds[4:12], ["reset", "place", "advance", "place", "advance", "move", "advance", "inspect"])
        self.assertEqual((attempts[5]["end"], attempts[7]["end"], attempts[6]["ticks"]), ("a", "b", 4))
        self.assertAlmostEqual(attempts[5]["eye"][1], 3.8)
        self.assertEqual(kinds.count("reset"), 4)

    def test_pairs_that_change_nothing_are_counted_not_listed(self):
        ctx = hall()
        run = game(lambda placed: (0, player([-6.0, 2.2, -4.0])))
        lines = sweep(ctx, "L1:4,6", MOVES, also_from=["L1:8,8"], run=run).splitlines()
        self.assertEqual(lines[2:], ["3 pairs end as without portals", "0 of 3 pairs carry the body through a portal"])
        self.assertEqual(len(run.calls[0]), 2)
        lines = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:12,10", run=run).splitlines()
        self.assertEqual(lines[1], "no portals: ends L1 cell (4.0, 6.0) on L1.a  GOAL")
        self.assertEqual(lines[-1], "3 reach the goal: the 3 that end as without portals")

    def test_named_ends_restrict_the_pairs(self):
        ctx = hall()
        run = game(lambda placed: (len(placed) // 2, player([-5.5, 2.2, -7.2])))
        text = sweep(ctx, "L1:4,6", MOVES, entries=["wall:L1:5,2:N:1"], exits=["floor:L1:9.5,6.5"], run=run)
        self.assertIn("3 portals in reach, 1 pairs", text)
        with self.assertRaisesRegex(
            ValueError, "ramp:L0:16,3 opens no portal: from L1 \\(4, 6\\): nothing within range"
        ):
            sweep(ctx, "L1:4,6", MOVES, entries=["ramp:L0:16,3"], run=run)

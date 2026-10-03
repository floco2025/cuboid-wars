import json
import tempfile
from pathlib import Path

from author_fixtures import hall
from config_fixtures import ConfigTestCase
from map_author.game import Start
from map_author.sweep import Goal, parse_moves, sweep, walk_in

MOVES = "move 0,-1 x40 jump; advance 20"
NORTH = [0.0, 0.0, 1.0]
PAD = "floor:L1:9.5,6.5"
WEST_PANEL = "wall:L1:5,2:N:1"
EAST_PANEL = "wall:L1:7,2:N:1"
# The hall's pad in world metres, and the west panel's x.
PAD_POINT = (-0.5, 2.2, -3.5)
WEST_X = -4.0
OVERLAP = -1


def player(position, support="ground", health=500.0):
    return {"position": position, "support": support, "health": health}


# A game in which the pad and the panel's lower storey open a portal, a move
# walks `path`, and an attempt ends where `ends` says for the portals it
# placed: its crossings and the player, or OVERLAP for a pair the game refuses.
def game(ends, path=(PAD_POINT,), refused=lambda feet: False):
    calls = []

    def run(ctx, spawn, actions):
        calls.append(actions)
        if actions[-1]["action"] == "probe":
            results = []
            for action in actions:
                if action["action"] == "teleport":
                    status = "rejected" if refused(tuple(action["feet"])) else "teleported"
                    results.append({"result": {"status": status}})
                    continue
                if action["action"] != "probe":
                    results.append({"result": {"status": "advanced"}})
                    continue
                shots = []
                for target in action["targets"]:
                    hit = {"position": target, "normal": NORTH if target[1] > 3 else [0.0, 1.0, 0.0]}
                    portal = {"position": target, "normal": hit["normal"], "yaw": 0.0}
                    shots.append(
                        {"target": target, "status": "placed", "hit": hit, "portal": portal}
                        if 2.1 < target[1] < 4.5
                        else {"target": target, "status": "no_surface"}
                    )
                results.append({"result": {"status": "probed", "shots": shots}})
            return {"steps": results}
        steps, placed = [], []
        initial = {"player": {"position": list(spawn), "support": "ground", "health": 500.0}}
        for action in actions:
            kind = action["action"]
            if kind == "reset":
                placed = []
            if kind == "place":
                placed.append(tuple(action["target"]))
            crossings, state = ends(tuple(placed))
            result = {"status": {"reset": "reset", "place": "submitted", "teleport": "teleported"}.get(kind, "done")}
            if kind == "teleport" and refused(tuple(action["feet"])):
                result = {"status": "rejected", "reason": "player overlaps geometry"}
            if crossings == OVERLAP and len(placed) == 2 and kind == "place":
                result = {"status": "rejected", "reason": "portal_overlap"}
            events = [{"kind": "player_step", "position": list(point)} for point in path] if kind == "move" else []
            if kind == "inspect":
                events = [{"kind": "player_portal_crossing"}] * max(crossings, 0)
            steps.append({"action": action, "result": result, "events": events, "state": {"player": state}})
        return {"initial": initial, "steps": steps}

    run.calls = calls
    return run


def out_of_the_west_panel(placed):
    return len(placed) == 2 and WEST_X in (placed[0][0], placed[1][0])


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
    def test_a_portal_on_the_way_is_paired_with_every_other_from_a_reset(self):
        ctx = hall()
        start = player([-6.0, 2.2, -4.0])

        def ends(placed):
            if out_of_the_west_panel(placed):
                return 1, player([-5.5, 2.2, -7.2], health=420.0)
            return (2, None) if len(placed) == 2 else (0, start)

        run = game(ends)
        lines = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:6,4", run=run).splitlines()
        self.assertEqual(lines[0], f"sweep from L1 (4, 6): 3 portals in reach, 1 on the way, 2 pairs, moves: {MOVES}")
        self.assertEqual(lines[1], "no portals: ends L1 cell (4.0, 6.0) on L1.a")
        self.assertEqual(
            lines[2], f"{PAD} + {WEST_PANEL}: crosses 1, ends L1 cell (4.5, 2.8) on L1.a, hp 420 of 500  GOAL"
        )
        self.assertEqual(lines[3], f"{PAD} + {EAST_PANEL}: crosses 2, DIES")
        self.assertEqual(lines[-2], "2 of 2 pairs carry the body through a portal")
        self.assertEqual(lines[-1], f"1 reach the goal: {PAD} + {WEST_PANEL}")
        probe, plain, attempts = run.calls
        self.assertEqual([action["action"] for action in probe], ["advance", "probe"])
        self.assertEqual([action["action"] for action in plain], ["reset", "advance", "move", "advance", "inspect"])
        kinds = [action["action"] for action in attempts]
        self.assertEqual(
            kinds[:9], ["reset", "advance", "place", "advance", "place", "advance", "move", "advance", "inspect"]
        )
        self.assertEqual((attempts[2]["end"], attempts[4]["end"], attempts[3]["ticks"]), ("a", "b", 4))
        self.assertAlmostEqual(attempts[2]["eye"][1], 3.8)
        self.assertEqual(kinds.count("reset"), 2)

    def test_pairs_reached_through_a_field_opened_by_the_starting_plate_are_tried(self):
        ctx = hall()
        base = game(lambda placed: (1 if len(placed) == 2 else 0, player([-5.5, 2.2, -7.2])))
        starts = []

        def run(ctx, spawn, actions):
            report = base(ctx, spawn, actions)
            ticks = 0
            for action, step in zip(actions, report["steps"], strict=True):
                kind = action["action"]
                if kind == "reset":
                    ticks = 0
                elif kind == "advance":
                    ticks += action["ticks"]
                elif kind in ("place", "probe", "move"):
                    starts.append(ticks)
                    if ticks < 6:
                        step["result"] = {"status": "rejected", "reason": "invalid_placement"}
            return report

        text = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:6,4", run=run)
        self.assertNotIn("not placed", text)
        self.assertIn("2 of 2 pairs carry the body through a portal", text)
        self.assertTrue(all(ticks >= 6 for ticks in starts), starts)

    def test_pairs_that_change_nothing_are_counted_not_listed(self):
        ctx = hall()
        run = game(lambda placed: (0, player([-6.0, 2.2, -4.0])))
        lines = sweep(ctx, "L1:4,6", MOVES, also_from=["L1:8,8"], run=run).splitlines()
        self.assertEqual(lines[2:], ["2 pairs end as without portals", "0 of 2 pairs carry the body through a portal"])
        self.assertEqual(len(run.calls[0]), 3)
        lines = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:12,10", run=run).splitlines()
        self.assertEqual(lines[1], "no portals: ends L1 cell (4.0, 6.0) on L1.a  GOAL")
        self.assertEqual(lines[-1], "2 reach the goal: the 2 that end as without portals")

    def test_pairs_the_game_refuses_as_overlapping_are_counted(self):
        ctx = hall()
        start = player([-6.0, 2.2, -4.0])
        run = game(lambda placed: (OVERLAP if out_of_the_west_panel(placed) else 0, start))
        lines = sweep(ctx, "L1:4,6", MOVES, run=run).splitlines()
        self.assertEqual(
            lines[2:],
            [
                "1 pairs overlap, so the game opens only one of the two",
                "1 pairs end as without portals",
                "0 of 2 pairs carry the body through a portal",
            ],
        )

    def test_moves_that_pass_no_portal_try_no_pair(self):
        run = game(lambda placed: (0, player([-6.0, 2.2, -4.0])), path=())
        lines = sweep(hall(), "L1:4,6", MOVES, run=run).splitlines()
        self.assertIn("3 portals in reach, 0 on the way, 0 pairs", lines[0])
        self.assertEqual(lines[-1], "the moves pass no portal a shot from here opens")
        self.assertEqual(len(run.calls), 2)

    def test_named_ends_are_tried_whatever_the_way(self):
        ctx = hall()
        run = game(lambda placed: (len(placed) // 2, player([-5.5, 2.2, -7.2])), path=())
        text = sweep(ctx, "L1:4,6", MOVES, entries=[WEST_PANEL], exits=[PAD], run=run)
        self.assertIn("3 portals in reach, 1 on the way, 1 pairs", text)
        with self.assertRaisesRegex(
            ValueError, "ramp:L0:16,3 opens no portal: from L1 \\(4, 6\\): nothing within range"
        ):
            sweep(ctx, "L1:4,6", MOVES, entries=["ramp:L0:16,3"], run=run)


class WalkInTests(ConfigTestCase):
    def test_every_exit_is_tried_from_one_floor_and_one_wall_entry(self):
        ctx = hall()
        start = player([-6.0, 2.2, -4.0])
        run = game(lambda placed: (1, player([-5.5, 2.2, -7.2])) if out_of_the_west_panel(placed) else (0, start))
        lines = walk_in(ctx, "L1:4,6", "L1:2,2:6,4", run=run).splitlines()
        self.assertEqual(lines[0], "walk-in from L1 (4, 6): 3 portals in reach, 3 exits")
        # Out of the pad: the west panel's two entries. Out of either panel: the pad's six.
        self.assertEqual(lines[1], "14 entries tried, 8 pass through a portal, 0 start inside geometry")
        self.assertEqual(lines[2], "SHORTCUT: 8 reach the goal without building speed")
        self.assertIn(f"  walking in at {WEST_PANEL}, out of {PAD}: ends L1 cell (4.5, 2.8) on L1.a", lines)
        self.assertIn(f"  stepping in at {PAD}, out of {WEST_PANEL}: ends L1 cell (4.5, 2.8) on L1.a", lines)
        self.assertIn(f"  walking north onto it at {PAD}, out of {WEST_PANEL}: ends L1 cell (4.5, 2.8) on L1.a", lines)
        resets = [action for action in run.calls[1] if action["action"] == "reset"]
        self.assertEqual(len(resets), 14)
        self.assertTrue(all(len(action["spawn"]) == 3 for action in resets))

    def test_a_goal_no_slow_entry_reaches_is_said_plainly(self):
        run = game(lambda placed: (1 if len(placed) == 2 else 0, player([0.0, 2.2, 0.0])))
        lines = walk_in(hall(), "L1:4,6", "L1:2,2:6,4", run=run).splitlines()
        self.assertEqual(lines[-1], "none reaches the goal")


# A route that crosses a portal on its way, then stands still.
ROUTE = {
    "spawn": [1.0, 2.2, 1.0],
    "actions": [
        {"action": "move", "direction": [0, -1], "ticks": 10},
        {"action": "inspect"},
        {"action": "advance", "ticks": 5},
    ],
}


def route_start(test, step: int) -> Start:
    folder = tempfile.TemporaryDirectory()
    test.addCleanup(folder.cleanup)
    path = Path(folder.name) / "route.json"
    path.write_text(json.dumps(ROUTE), encoding="utf-8")
    return Start.after(f"{path}:{step}")


class AfterTests(ConfigTestCase):
    def test_a_route_is_replayed_through_the_named_step(self):
        start = route_start(self, 1)
        self.assertEqual(start.prefix, tuple(ROUTE["actions"][:2]))
        self.assertEqual(start.spawn, (1.0, 2.2, 1.0))
        self.assertEqual(start.label, " after step 1 of route.json")
        with self.assertRaisesRegex(ValueError, "no step 3"):
            route_start(self, 3)
        with self.assertRaisesRegex(ValueError, "is not <route.json>:<step>"):
            Start.after("route.json")

    def test_every_attempt_starts_from_the_routes_state_and_counts_only_its_own_crossings(self):
        ctx = hall()
        start = player([-6.0, 2.2, -4.0])

        # Nothing placed is the route's own crossing, which no attempt counts.
        def ends(placed):
            if not placed:
                return 1, start
            if out_of_the_west_panel(placed):
                return 1, player([-5.5, 2.2, -7.2])
            return 0, start

        run = game(ends)
        lines = sweep(ctx, "L1:4,6", MOVES, goal="L1:2,2:6,4", run=run, start=route_start(self, 1)).splitlines()
        self.assertTrue(lines[0].startswith("sweep from L1 (4, 6) after step 1 of route.json: "), lines[0])
        self.assertIn(f"{PAD} + {WEST_PANEL}: crosses 1, ends L1 cell (4.5, 2.8) on L1.a  GOAL", lines)
        probe, plain, attempts = run.calls
        self.assertEqual([action["action"] for action in probe], ["move", "inspect", "teleport", "advance", "probe"])
        self.assertEqual(probe[2]["feet"], [-6.0, 2.35, -4.0])
        begin = ["reset", "move", "inspect", "teleport", "advance"]
        self.assertEqual([action["action"] for action in plain][:5], begin)
        self.assertEqual([action["action"] for action in attempts][:7], [*begin, "place", "advance"])

    def test_a_teleport_the_game_refuses_is_a_start_inside_geometry(self):
        ctx = hall()
        stand = (-6.0, 2.35, -4.0)
        nowhere = game(lambda placed: (0, None), refused=lambda feet: True)
        with self.assertRaisesRegex(ValueError, "leaves no body to stand at L1 \\(4, 6\\)"):
            walk_in(ctx, "L1:4,6", "L1:2,2:6,4", run=nowhere, start=route_start(self, 2))
        walled = game(lambda placed: (0, player(list(stand))), refused=lambda feet: feet != stand)
        lines = walk_in(ctx, "L1:4,6", "L1:2,2:6,4", run=walled, start=route_start(self, 2)).splitlines()
        self.assertEqual(lines[1], "0 entries tried, 0 pass through a portal, 14 start inside geometry")
        teleports = [action for action in walled.calls[1] if action["action"] == "teleport"]
        self.assertEqual(len(teleports), 14)

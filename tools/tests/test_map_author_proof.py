import unittest

from map_author.frame import GridFrame
from map_author.proof import summarize

FRAME = GridFrame(cols=20, rows=20, cell=2.0, level_height=2.2, wall_thickness=0.2, floor_thickness=0.2)


def player(position, support="ground", health=500.0, checkpoint=0, speed=False, low_gravity=False):
    return {
        "position": position,
        "support": support,
        "health": health,
        "checkpoint": checkpoint,
        "speed": speed,
        "low_gravity": low_gravity,
        "keys": [],
    }


def state(player_state, switches=(), fields=()):
    return {"tick": 0, "player": player_state, "active_switches": list(switches), "open_fields": list(fields)}


def step(index, action, result, events=(), player_state=None, **state_fields):
    return {
        "index": index,
        "action": action,
        "result": result,
        "events": list(events),
        "state": state(player_state, **state_fields),
    }


def report():
    return {
        "initial": state(player([-10.0, 4.4, 0.0])),
        "steps": [
            step(
                0,
                {"action": "portal", "end": "a"},
                {"status": "rejected", "reason": "invalid_placement"},
                player_state=player([-10.0, 4.4, 0.0]),
            ),
            step(
                1,
                {"action": "move", "direction": [1, 0], "ticks": 40, "crouch": False, "jump": True},
                {"status": "simulated", "ticks": 40},
                [
                    {"kind": "jump", "accepted": True, "tick": 3},
                    {"kind": "player_step", "tick": 3},
                    {
                        "kind": "player_portal_crossing",
                        "tick": 12,
                        "velocity_before": [8.0, -3.0, 0.0],
                        "velocity_after": [0.0, -3.0, 8.0],
                    },
                    {"kind": "player_landed", "tick": 30, "impact_speed": 6.5},
                    {"kind": "checkpoint_reached", "tick": 30, "checkpoint": 1},
                    {"kind": "item_collected", "tick": 35, "item": "speed"},
                ],
                player_state=player([4.0, 6.6, 3.0], checkpoint=1, speed=True),
                switches=["gate"],
            ),
            step(
                2,
                {"action": "check", "min": [2.0, 6.5, 2.0], "max": [6.0, 6.7, 6.0], "grounded": True},
                {"status": "failed", "reason": "not_grounded", "inside": True, "grounded": False},
                player_state=player([4.0, 7.3, 3.0], support="airborne", checkpoint=1, speed=True),
            ),
            step(
                3,
                {"action": "advance", "ticks": 60},
                {"status": "advanced"},
                [
                    {"kind": "player_fall_damage", "tick": 70, "health": 420.0},
                    {"kind": "player_died", "tick": 71},
                    {"kind": "player_relocated", "tick": 131},
                ],
                player_state=player([-9.0, 4.4, 1.0], health=500.0, checkpoint=1),
            ),
        ],
    }


class ProofSummaryTests(unittest.TestCase):
    def test_one_line_per_action_with_the_events_that_matter(self):
        lines = summarize(report(), FRAME).splitlines()
        self.assertEqual(lines[0], "#0   portal a                           REJECTED invalid_placement")
        self.assertEqual(
            lines[1],
            "#1   move (1,0) x40 jump                simulated 40 ticks   t12 crossing v(8.0, -3.0, 0.0) -> "
            "v(0.0, -3.0, 8.0) | t30 landed at 6.5 m/s | t30 checkpoint 1 | t35 collected speed",
        )
        self.assertEqual(lines[2], " " * 39 + "-> (4.00, 6.60, 3.00)  L3 cell (12, 11)  ground  hp 500")
        self.assertEqual(lines[3], "#2   check [(2.0, 6.5, 2.0) .. (6.0, 6.7, 6.0)] FAILED not_grounded")
        self.assertEqual(
            lines[4], " " * 39 + "-> (4.00, 7.30, 3.00)  between L3/L4 (y=7.30) cell (12, 11)  airborne  hp 500"
        )
        self.assertEqual(
            lines[5],
            "#3   advance x60                        advanced   t70 fall damage hp 420 | t71 died | t131 relocated",
        )

    def test_probes_and_placements_read_as_shots(self):
        shots = [{"status": "placed"}, {"status": "no_fit"}, {"status": "placed"}]
        steps = [
            step(0, {"action": "probe", "eye": None, "targets": [[0, 0, 0]] * 3}, {"status": "probed", "shots": shots}),
            step(
                1,
                {"action": "place", "end": "b", "eye": [0.0, 1.6, 0.0], "target": [2.0, 3.5, -7.9]},
                {"status": "submitted", "portal": {"position": [2.0, 3.6, -7.9], "normal": [0.0, 0.0, 1.0]}},
            ),
        ]
        lines = summarize({"initial": state(None), "steps": steps}, FRAME).splitlines()
        self.assertEqual(lines[0], "#0   probe x3                           2 of 3 shots open a portal")
        self.assertEqual(
            lines[1], "#1   place b at (2, 3.5, -7.9)          submitted at (2.0, 3.6, -7.9) normal (0, 0, 1)"
        )

    def test_the_footer_counts_what_happened(self):
        lines = summarize(report(), FRAME).splitlines()
        self.assertEqual(
            lines[-2],
            "final -> (-9.00, 4.40, 1.00)  L2 cell (5, 10)  ground  hp 500  cp 1  speed no  low gravity no  "
            "keys []  switches []  open fields []",
        )
        self.assertEqual(
            lines[-1],
            "checks 0/1 passed   crossings 1   checkpoints 1   deaths 1   fall damage 1   fireworks no",
        )

    def test_a_dead_player_has_no_final_position(self):
        text = summarize(
            {"initial": state(None), "steps": [step(0, {"action": "inspect"}, {"status": "inspected"})]}, FRAME
        )
        self.assertEqual(text.splitlines()[0], "#0   inspect                            inspected")
        self.assertIn("final: player dead", text)

from author_fixtures import hall
from config_fixtures import ConfigTestCase
from map_author.shots import candidates, shots, stand_at

WALLS = ["wall:L1:5,2:N:1", "wall:L1:7,2:N:1"]
# The panel's upper storey: a portal aimed there is the game's to move down.
UPPER = ["wall:L2:5,2:N:1", "wall:L2:7,2:N:1"]
PAD = "floor:L1:9.5,6.5"


def answers(by_target):
    def run(ctx, spawn, actions):
        (action,) = actions
        return {"steps": [{"result": {"status": "probed", "shots": [by_target(t) for t in action["targets"]]}}]}

    return run


class CandidateTests(ConfigTestCase):
    def test_surfaces_are_sampled_where_portals_fit_side_by_side(self):
        found = candidates(hall(deck=True))
        targets = {target.spec: target.point for target in found}
        normals = {target.spec: target.normal for target in found}
        self.assertEqual(normals[WALLS[0]], (0.0, 0.0, 1.0))
        self.assertEqual(normals["ceiling:L2:15.5,11.5"], (0.0, -1.0, 0.0))
        for value, wanted in zip(normals["ramp:L0:16,3"], (-0.482, 0.876, 0.0), strict=True):
            self.assertAlmostEqual(value, wanted, places=3)
        self.assertEqual(
            list(targets), [*WALLS, *UPPER, PAD, "floor:L2:15.5,11.5", "ceiling:L2:15.5,11.5", "ramp:L0:16,3"]
        )
        # Cells to world on a 20x20 grid of 1 m; a wall is aimed just above its base.
        for point, expected in (
            (targets[WALLS[0]], (-4.0, 2.2 + 1.378 + 0.02, -7.9)),
            (targets[PAD], (-0.5, 2.2, -3.5)),
            (targets["ceiling:L2:15.5,11.5"], (5.5, 4.2, 1.5)),
            (targets["ramp:L0:16,3"], (6.0, 1.1, -7.0)),
        ):
            for value, wanted in zip(point, expected, strict=True):
                self.assertAlmostEqual(value, wanted)

    def test_a_stand_is_feet_on_a_level_with_the_eye_above(self):
        stand = stand_at(hall(), "L1:4,6")
        self.assertEqual((stand.label, stand.feet), ("L1 (4, 6)", (-6.0, 2.2, -4.0)))
        self.assertAlmostEqual(stand.eye[1], 3.8)


class ShotTests(ConfigTestCase):
    def test_each_target_gets_the_games_verdict(self):
        ctx = hall(deck=True)
        targets = {tuple(target.point): target.spec for target in candidates(ctx)}

        def game(point):
            spec = targets[tuple(point)]
            hit = {"position": point, "normal": [0.0, 0.0, 1.0]}
            if spec == WALLS[0]:
                portal = {"position": [point[0], point[1] + 0.125, point[2]], "normal": [0.0, 0.0, 1.0], "yaw": 0.0}
                return {"target": point, "status": "placed", "hit": hit, "portal": portal}
            if spec == WALLS[1]:
                return {"target": point, "status": "incompatible_material", "hit": hit}
            if spec.startswith("floor"):
                return {"target": point, "status": "no_fit", "hit": hit}
            if spec.startswith("ceiling"):
                wall = {"position": [2.1, 3.0, -3.5], "normal": [-1.0, 0.0, 0.0]}
                return {"target": point, "status": "incompatible_material", "hit": wall}
            return {"target": point, "status": "no_surface"}

        def facing(point):
            spec = targets[tuple(point)]
            normal = [0.0, 1.0, 0.0] if spec.startswith(("floor", "ceiling", "ramp")) else [0.0, 0.0, 1.0]
            return {**game(point), **({"hit": {"position": point, "normal": normal}} if "floor" in spec else {})}

        overview = shots(ctx, "L1:4,6", run=answers(facing)).splitlines()
        self.assertEqual(
            overview[1:],
            [
                "  L1 wall on row 2 facing south: 1 of 2 open  wall:L1:5,2:N:1",
                "  not opening: 4 out of sight, 2 no fit, 1 fizzles",
                "1 of 8 open a portal where aimed",
            ],
        )
        lines = shots(ctx, "L1:4,6", list(targets.values()), run=answers(facing)).splitlines()
        self.assertEqual(lines[0], "shots from L1 (4, 6), eye world (-6.00, 3.80, -4.00): 8 targets")
        self.assertEqual(
            lines[1], "  wall:L1:5,2:N:1       opens at (-4.00, 3.72, -7.90) normal (0, 0, 1), nudged 0.12 m"
        )
        self.assertIn("fizzles: the aperture would cover a surface that takes no portal", lines[2])
        self.assertIn("nothing within range", lines[3])
        self.assertIn("no fit: nothing within the nudge", lines[5])
        self.assertEqual(
            lines[7],
            "  ceiling:L2:15.5,11.5  BLOCKED: the shot lands at (2.10, 3.00, -3.50) on a surface facing (-1, 0, 0) "
            "that takes no portal",
        )
        self.assertIn("nothing within range", lines[8])
        self.assertEqual(lines[9], "1 of 8 open a portal where aimed")

    def test_named_surfaces_replace_the_whole_list(self):
        ctx = hall()
        asked = []

        def run(ctx, spawn, actions):
            asked.append((spawn, actions))
            shots = [{"target": t, "status": "no_surface"} for t in actions[0]["targets"]]
            return {"steps": [{"result": {"status": "probed", "shots": shots}}]}

        text = shots(ctx, "L1:4,6", ["wall:L1:3,2:N", "ramp:L0:16,3"], run=run)
        ((spawn, (action,)),) = asked
        self.assertEqual(spawn, (-6.0, 2.2, -4.0))
        self.assertEqual((action["action"], len(action["targets"])), ("probe", 2))
        self.assertAlmostEqual(action["eye"][1], 3.8)
        self.assertIn("wall:L1:3,2:N", text)
        with self.assertRaisesRegex(ValueError, "not a wall or floor spec"):
            shots(ctx, "L1:4,6", ["roof:L1:1,1"], run=run)

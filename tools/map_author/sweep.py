"""Where moves end for every pair of portals a standing point reaches, in the game itself."""

from __future__ import annotations

import re
from dataclasses import dataclass
from math import dist

from . import game
from .context import MapContext
from .describe import labelled_platforms
from .index import MapIndex
from .shots import Point, Shot, Stand, _point, candidates, named_targets, probe, stand_at, verdict

MOVE_RE = re.compile(r"^move\s+(-?[0-9.]+)\s*,\s*(-?[0-9.]+)\s+x(\d+)((?:\s+(?:jump|crouch))*)$")
ADVANCE_RE = re.compile(r"^advance\s+x?(\d+)$")
GOAL_RE = re.compile(r"^L(\d+):(-?[0-9.]+),(-?[0-9.]+):(-?[0-9.]+),(-?[0-9.]+)$")
# Two ends this close are where the moves end without portals.
SAME_PLACE = 0.05
# Two portals on one surface this close together are one place to put a portal.
SAME_PORTAL = 0.25


# `move <x>,<z> x<ticks> [jump] [crouch]; advance <ticks>; ...`, the runner's own actions.
def parse_moves(text: str) -> list[dict]:
    actions = []
    for part in filter(None, (part.strip() for part in text.split(";"))):
        if match := MOVE_RE.match(part):
            x, z, ticks, flags = match.groups()
            actions.append(
                {
                    "action": "move",
                    "direction": [float(x), float(z)],
                    "ticks": int(ticks),
                    "jump": "jump" in flags,
                    "crouch": "crouch" in flags,
                }
            )
        elif match := ADVANCE_RE.match(part):
            actions.append({"action": "advance", "ticks": int(match.group(1))})
        else:
            raise ValueError(f"move {part!r} is not `move <x>,<z> x<ticks> [jump] [crouch]` or `advance <ticks>`")
    if not actions:
        raise ValueError("a sweep needs moves to try each pair with")
    return actions


# `L<level>:<c0>,<r0>:<c1>,<r1>` in cells: standing on that level inside the rectangle.
@dataclass(frozen=True)
class Goal:
    level: int
    c0: float
    r0: float
    c1: float
    r1: float

    @classmethod
    def parse(cls, spec: str) -> Goal:
        match = GOAL_RE.match(spec)
        if not match:
            raise ValueError(f"goal {spec!r} is not L<level>:<c0>,<r0>:<c1>,<r1> in cells")
        level, *corners = match.groups()
        return cls(int(level), *map(float, corners))

    def holds(self, ctx: MapContext, player: dict | None) -> bool:
        if player is None or player["support"] != "ground":
            return False
        x, y, z = player["position"]
        gx, gz = ctx.frame.world_to_grid(x, z)
        return ctx.frame.level_of_y(y) == self.level and self.c0 <= gx <= self.c1 and self.r0 <= gz <= self.r1


# A portal a shot opens: the shot to repeat, and where the aperture is.
@dataclass(frozen=True)
class Placement:
    label: str
    eye: Point
    target: Point
    position: Point
    normal: Point

    def action(self, end: str) -> dict:
        return {"action": "place", "end": end, "eye": list(self.eye), "target": list(self.target)}


# The distinct portals the shots open: one aimed where it landed keeps its
# surface's name, and one that met another surface first is named by where it
# is unless a named one is already there.
def placements(shots: list[Shot]) -> list[Placement]:
    found: list[Placement] = []
    for shot in sorted((shot for shot in shots if shot.status == "placed"), key=lambda shot: not shot.on_target):
        position, normal = tuple(shot.portal["position"]), tuple(shot.portal["normal"])
        if any(dist(position, other.position) <= SAME_PORTAL and dist(normal, other.normal) <= 0.1 for other in found):
            continue
        label = shot.target.spec if shot.on_target else f"portal at {_point(position)}"
        found.append(Placement(label, shot.stand.eye, shot.target.point, position, normal))
    return found


# What one attempt did, read from its steps of the report.
@dataclass(frozen=True)
class Outcome:
    refused: str | None
    crossings: int
    died: bool
    player: dict | None

    @classmethod
    def read(cls, steps: list[dict]) -> Outcome:
        refused = next(
            (
                step["result"].get("reason", step["result"]["status"])
                for step in steps
                if step["action"]["action"] == "place" and step["result"]["status"] != "submitted"
            ),
            None,
        )
        events = [event["kind"] for step in steps for event in step["events"]]
        return cls(
            refused,
            events.count("player_portal_crossing"),
            "player_died" in events,
            steps[-1]["state"]["player"],
        )


def _attempt(pair: tuple[Placement, Placement] | None, moves: list[dict], wait: int) -> list[dict]:
    actions = [{"action": "reset"}]
    for end, placement in zip("ab", pair or ()):
        actions += [placement.action(end), {"action": "advance", "ticks": wait}]
    return [*actions, *moves, {"action": "inspect"}]


def _end(ctx: MapContext, labels: dict, outcome: Outcome) -> str:
    if outcome.player is None:
        return "DIES"
    x, y, z = outcome.player["position"]
    gx, gz = ctx.frame.world_to_grid(x, z)
    level = ctx.frame.level_of_y(y)
    where = f"{ctx.frame.describe_y(y)} cell ({gx:.1f}, {gz:.1f})"
    if outcome.player["support"] != "ground":
        return f"still in the air at {where}" + (", after dying" if outcome.died else "")
    platform = labels.get((level, (int(gx // 1), int(gz // 1))), "no platform")
    return f"ends {where} on {platform}" + (", after dying" if outcome.died else "")


def sweep(
    ctx: MapContext,
    stand_spec: str,
    moves_text: str,
    *,
    entries=(),
    exits=(),
    also_from=(),
    goal: str | None = None,
    run=game.run,
) -> str:
    moves = parse_moves(moves_text)
    target = Goal.parse(goal) if goal else None
    stands: list[Stand] = [stand_at(ctx, spec) for spec in (stand_spec, *also_from)]
    known = candidates(ctx)
    named = named_targets(ctx, [*entries, *exits], known)
    targets = list({target.spec: target for target in (*known, *named)}.values())
    if not targets:
        raise ValueError("the map has no portalable surface")
    shots = [shot for results in probe(ctx, stands, targets, run) for shot in results]
    found = placements(shots)

    def chosen(specs) -> list[Placement]:
        if not specs:
            return found
        picked = []
        for spec in specs:
            matches = [placement for placement in found if placement.label == spec]
            if not matches:
                tried = "; ".join(f"from {s.stand.label}: {verdict(s)}" for s in shots if s.target.spec == spec)
                raise ValueError(f"{spec} opens no portal: {tried}")
            picked.extend(matches)
        return picked

    # A pair works the same whichever end is which, so each is tried once.
    unordered: dict[frozenset, tuple[Placement, Placement]] = {}
    for a in chosen(entries):
        for b in chosen(exits):
            if a is not b:
                unordered.setdefault(frozenset((id(a), id(b))), (a, b))
    pairs = list(unordered.values())
    wait = game.cooldown_ticks(ctx)
    blocks = [_attempt(None, moves, wait), *(_attempt(pair, moves, wait) for pair in pairs)]
    steps = iter(run(ctx, stands[0].feet, [action for block in blocks for action in block])["steps"])
    baseline, *outcomes = (Outcome.read([next(steps) for _ in block]) for block in blocks)

    labels = {(level, cell): label for label, level, cells, _ in labelled_platforms(MapIndex(ctx)) for cell in cells}
    lines = [
        f"sweep from {stands[0].label}: {len(found)} portals in reach, {len(pairs)} pairs, moves: {moves_text.strip()}",
        f"no portals: {_end(ctx, labels, baseline)}" + (_goal(ctx, target, baseline)),
    ]
    entered, unmoved, reached = 0, 0, []
    for (a, b), outcome in zip(pairs, outcomes):
        pair = f"{a.label} + {b.label}"
        if outcome.refused:
            lines.append(f"{pair}: not placed ({outcome.refused})")
            continue
        if (
            outcome.crossings == 0
            and (outcome.player is None) == (baseline.player is None)
            and (outcome.player is None or dist(outcome.player["position"], baseline.player["position"]) <= SAME_PLACE)
        ):
            unmoved += 1
            continue
        entered += outcome.crossings > 0
        if target and target.holds(ctx, outcome.player):
            reached.append(pair)
        crossed = f"crosses {outcome.crossings}" if outcome.crossings else "no crossing"
        lines.append(f"{pair}: {crossed}, {_end(ctx, labels, outcome)}{_goal(ctx, target, outcome)}")
    if unmoved:
        lines.append(f"{unmoved} pairs end as without portals{_goal(ctx, target, baseline)}")
    lines.append(f"{entered} of {len(pairs)} pairs carry the body through a portal")
    if target:
        # The moves alone reaching the goal take every pair that changes nothing there too.
        idle = unmoved if target.holds(ctx, baseline.player) else 0
        named = [*reached, *([f"the {idle} that end as without portals"] if idle else [])]
        lines.append(f"{len(reached) + idle} reach the goal" + (": " + "; ".join(named) if named else ""))
    return "\n".join(lines)


def _goal(ctx: MapContext, goal: Goal | None, outcome: Outcome) -> str:
    return "  GOAL" if goal and goal.holds(ctx, outcome.player) else ""

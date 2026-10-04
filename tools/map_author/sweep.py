"""Where moves end for every pair of portals a standing point reaches, in the game itself."""

from __future__ import annotations

import re
from dataclasses import dataclass
from math import ceil, dist

from map_editor.portal_surfaces import PORTAL_RIM_SCALE

from . import game
from .context import MapContext
from .describe import labelled_platforms
from .index import MapIndex
from .shots import STAND_SETTLE_TICKS, Point, Shot, Stand, candidates, named_targets, probe, stand_at, verdict

MOVE_RE = re.compile(r"^move\s+(-?[0-9.]+)\s*,\s*(-?[0-9.]+)\s+x(\d+)((?:\s+(?:jump|crouch))*)$")
ADVANCE_RE = re.compile(r"^advance\s+x?(\d+)$")
GOAL_RE = re.compile(r"^L(\d+):(-?[0-9.]+),(-?[0-9.]+):(-?[0-9.]+),(-?[0-9.]+)$")
# Two ends this close are where the moves end without portals.
SAME_PLACE = 0.05
# Two portals on one surface this close together are one place to put a portal.
SAME_PORTAL = 0.25
# How far past an aperture's length a path still counts as passing a portal:
# the funnel draws a falling body in from about that far.
PATH_MARGIN = 1.0
# A body's middle above its feet.
BODY_MIDDLE = 0.9
# How far from a portal a slow entry starts walking, and running before a jump.
WALK_UP = 1.6
RUN_UP = 3.2
# The ticks a slow entry is given to come to rest.
SETTLE_TICKS = 75
# An entry this far from the exit cannot overlap it.
CLEAR_OF_EXIT = 3.5


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


# PRESSURE_PLATE_HEIGHT in common/src/constants.rs: a body on a plate stands this far above its level.
PLATE_HEIGHT = 0.11


# `L<level>:<c0>,<r0>:<c1>,<r1>` in cells: standing on that level inside the rectangle, on a plate or not.
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
        on_level = self.level in (ctx.frame.level_of_y(y), ctx.frame.level_of_y(y - PLATE_HEIGHT))
        return on_level and self.c0 <= gx <= self.c1 and self.r0 <= gz <= self.r1


# A portal a shot opens: the shot to repeat, where the aperture is, and
# whether a floor at its foot lets a body walk into it.
@dataclass(frozen=True)
class Placement:
    label: str
    eye: Point
    target: Point
    position: Point
    normal: Point
    walk_up: bool = False

    @property
    def floor(self) -> bool:
        return self.normal[1] > 0.7

    @property
    def wall(self) -> bool:
        return abs(self.normal[1]) < 0.3

    def action(self, end: str) -> dict:
        return {"action": "place", "end": end, "eye": list(self.eye), "target": list(self.target)}


# The distinct portals the shots open where they were aimed. A shot that met
# another surface first opens one there too, which that surface's own samples
# stand for.
def placements(shots: list[Shot]) -> list[Placement]:
    found: list[Placement] = []
    for shot in shots:
        if not shot.opens:
            continue
        position, normal = tuple(shot.portal["position"]), tuple(shot.portal["normal"])
        if any(dist(position, other.position) <= SAME_PORTAL and dist(normal, other.normal) <= 0.1 for other in found):
            continue
        target = shot.target
        found.append(Placement(target.spec, shot.stand.eye, target.point, position, normal, target.walk_up))
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


def _attempt(
    start: game.Start, feet, pair: tuple[Placement, Placement] | None, moves: list[dict], wait: int
) -> list[dict]:
    # Shots and the baseline both start with the plate and pickup state the probe saw.
    actions = [*start.begin(feet), {"action": "advance", "ticks": STAND_SETTLE_TICKS}]
    for end, placement in zip("ab", pair or ()):
        actions += [placement.action(end), {"action": "advance", "ticks": wait}]
    return [*actions, *moves, {"action": "inspect"}]


# `full` is the health the attempt began with: less is named.
def _end(ctx: MapContext, labels: dict, outcome: Outcome, full: float | None = None) -> str:
    if outcome.player is None:
        return "DIES"
    x, y, z = outcome.player["position"]
    gx, gz = ctx.frame.world_to_grid(x, z)
    level = ctx.frame.level_of_y(y)
    where = f"{ctx.frame.describe_y(y)} cell ({gx:.1f}, {gz:.1f})"
    health = outcome.player.get("health")
    hurt = f", hp {health:.0f} of {full:.0f}" if full is not None and health is not None and health < full else ""
    after = hurt + (", after dying" if outcome.died else "")
    if outcome.player["support"] != "ground":
        return f"still in the air at {where}{after}"
    platform = labels.get((level, (int(gx // 1), int(gz // 1))), "no platform")
    return f"ends {where} on {platform}{after}"


# The portals the standing points open, and those named for either end of a
# pair, or all of them.
def _reach(ctx: MapContext, stand_spec: str, also_from, entries, exits, run, start: game.Start):
    stands: list[Stand] = [stand_at(ctx, spec) for spec in (stand_spec, *also_from)]
    known = candidates(ctx)
    named = named_targets(ctx, [*entries, *exits], known)
    targets = list({target.spec: target for target in (*known, *named)}.values())
    if not targets:
        raise ValueError("the map has no portalable surface")
    shots = [shot for results in probe(ctx, stands, targets, run, start) for shot in results]
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

    return stands, found, chosen(entries), chosen(exits)


# Each pair once: it works the same whichever end is which.
def _pairs(firsts: list[Placement], seconds: list[Placement]) -> list[tuple[Placement, Placement]]:
    unordered: dict[frozenset, tuple[Placement, Placement]] = {}
    for a in firsts:
        for b in seconds:
            if a is not b:
                unordered.setdefault(frozenset((id(a), id(b))), (a, b))
    return list(unordered.values())


# The portals a path comes near enough to enter: its feet, or its middle,
# within an aperture's length of the centre.
def _on_path(found: list[Placement], steps: list[dict], reach: float) -> list[Placement]:
    feet = [event["position"] for step in steps for event in step["events"] if event["kind"] == "player_step"]
    points = [point for x, y, z in feet for point in ((x, y, z), (x, y + BODY_MIDDLE, z))]
    return [portal for portal in found if any(dist(portal.position, point) <= reach for point in points)]


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
    start: game.Start = game.Start(),
) -> str:
    moves = parse_moves(moves_text)
    target = Goal.parse(goal) if goal else None
    stands, found, firsts, seconds = _reach(ctx, stand_spec, also_from, entries, exits, run, start)
    wait = game.cooldown_ticks(ctx)
    feet = stands[0].feet
    begun = len(start.begin(feet))
    plain = run(ctx, start.spawn_for(feet), _attempt(start, feet, None, moves, wait))
    full = (plain["steps"][begun - 1]["state"]["player"] or {}).get("health")
    baseline = Outcome.read(plain["steps"][begun:])
    # Only a portal on the body's way is ever entered, so unless the ends are
    # named, one end of every pair is such a portal.
    reach = 2 * ctx.settings.portal_half_height + PATH_MARGIN
    near = firsts if entries else _on_path(firsts, plain["steps"][begun:], reach)
    pairs = _pairs(near, seconds)
    labels = {(level, cell): label for label, level, cells, _ in labelled_platforms(MapIndex(ctx)) for cell in cells}
    lines = [
        f"sweep from {stands[0].label}{start.label}: {len(found)} portals in reach, {len(near)} on the way, "
        f"{len(pairs)} pairs, "
        f"moves: {moves_text.strip()}",
        f"no portals: {_end(ctx, labels, baseline, full)}" + (_goal(ctx, target, baseline)),
    ]
    if not pairs:
        return "\n".join([*lines, "the moves pass no portal a shot from here opens"])
    blocks = [_attempt(start, feet, pair, moves, wait) for pair in pairs]
    steps = iter(run(ctx, start.spawn_for(feet), [action for block in blocks for action in block])["steps"])
    outcomes = [Outcome.read([next(steps) for _ in block][begun:]) for block in blocks]
    entered, unmoved, overlapping, reached = 0, 0, 0, []
    for (a, b), outcome in zip(pairs, outcomes):
        pair = f"{a.label} + {b.label}"
        if outcome.refused == "portal_overlap":
            overlapping += 1
            continue
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
        lines.append(f"{pair}: {crossed}, {_end(ctx, labels, outcome, full)}{_goal(ctx, target, outcome)}")
    if overlapping:
        lines.append(f"{overlapping} pairs overlap, so the game opens only one of the two")
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


def _move(dx: float, dz: float, ticks: int, jump: bool = False) -> dict:
    return {"action": "move", "direction": [dx, dz], "ticks": ticks, "jump": jump, "crouch": False}


# The ways a body gets into a portal without building speed first, each a
# name, a feet position to start from, and the moves: stepping, dropping from
# a jump's apex, or walking onto a floor portal, and walking or running and
# jumping into a wall portal from the floor its rim rests on.
def slow_entries(ctx: MapContext, portal: Placement) -> list[tuple[str, Point, list[dict]]]:
    x, y, z = portal.position
    nx, ny, nz = portal.normal
    settle = {"action": "advance", "ticks": SETTLE_TICKS}
    physics = ctx.settings.physics
    if ny > 0.7:
        apex = physics["player"]["jump_speed"] ** 2 / (2 * physics["gravity"])
        entries = [("stepping in", (x, y + 0.3, z), [settle]), ("dropping in from a jump", (x, y + apex, z), [settle])]
        for name, (dx, dz) in (("north", (0, -1)), ("south", (0, 1)), ("east", (1, 0)), ("west", (-1, 0))):
            start = x - dx * WALK_UP, y, z - dz * WALK_UP
            entries.append((f"walking {name} onto it", start, [_move(dx, dz, 12), settle]))
        return entries
    if abs(ny) < 0.3:
        feet = y - ctx.settings.portal_half_height * PORTAL_RIM_SCALE
        near = x + nx * WALK_UP, feet, z + nz * WALK_UP
        far = x + nx * RUN_UP, feet, z + nz * RUN_UP
        return [
            ("walking in", near, [_move(-nx, -nz, 14), settle]),
            ("running and jumping in", far, [_move(-nx, -nz, 10), _move(-nx, -nz, 14, jump=True), settle]),
        ]
    return []


# How much room a portal leaves a body to try every slow entry: the cells
# with a slab within a run-up of it on its level, and none if a ceiling is
# too low over a floor portal to drop into it from a jump's height.
def _room_around(ctx: MapContext, index: MapIndex, portal: Placement) -> int:
    x, y, z = portal.position
    foot = y if portal.floor else y - ctx.settings.portal_half_height * PORTAL_RIM_SCALE
    level = ctx.frame.level_of_y(round(foot / ctx.frame.level_height) * ctx.frame.level_height)
    if level is None or level >= index.count:
        return 0
    col, row = ctx.frame.cell_of_world(x + portal.normal[0], z + portal.normal[2])
    physics = ctx.settings.physics
    clear = physics["player"]["jump_speed"] ** 2 / (2 * physics["gravity"]) + physics["body"]["height"]
    above = range(level + 1, min(level + ceil(clear / ctx.frame.level_height) + 1, index.count))
    if portal.floor and any(index.slab(storey, (col, row)) for storey in above):
        return 0
    reach = round(RUN_UP / ctx.frame.cell)
    return sum(
        index.slab(level, (col + dc, row + dr)) for dc in range(-reach, reach + 1) for dr in range(-reach, reach + 1)
    )


# Whether a pair of portals takes a body to the goal that only walks, steps,
# or hops into one of them: a shortcut no sweep of chosen moves would try.
def walk_in(
    ctx: MapContext,
    stand_spec: str,
    goal: str,
    *,
    entries=(),
    exits=(),
    also_from=(),
    run=game.run,
    start: game.Start = game.Start(),
) -> str:
    target = Goal.parse(goal)
    stands, found, firsts, seconds = _reach(ctx, stand_spec, also_from, entries, exits, run, start)
    wait = game.cooldown_ticks(ctx)
    # Where a slow body leaves a portal depends on the exit and on how it
    # went in, not on which floor or wall it went into: one floor and one
    # wall with a floor at its foot stand for every entry, unless named.
    if not entries:
        index = MapIndex(ctx)
        roomy = sorted(firsts, key=lambda portal: -_room_around(ctx, index, portal))
        firsts = [portal for portal in roomy if portal.floor][:2] + [
            portal for portal in roomy if portal.wall and portal.walk_up
        ][:2]
    attempts, actions = [], []
    for exit in seconds:
        tried = set()
        for entry in firsts:
            kind = entry.floor
            if entry is exit or kind in tried or dist(entry.position, exit.position) < CLEAR_OF_EXIT:
                continue
            tried.add(kind)
            for name, spawn, moves in slow_entries(ctx, entry):
                block = [
                    *start.begin(spawn),
                    entry.action("a"),
                    {"action": "advance", "ticks": wait},
                    exit.action("b"),
                    {"action": "advance", "ticks": wait},
                    *moves,
                    {"action": "inspect"},
                ]
                attempts.append((entry, exit, name, len(block)))
                actions.extend(block)
    lines = [f"walk-in from {stands[0].label}{start.label}: {len(found)} portals in reach, {len(seconds)} exits"]
    if not attempts:
        return "\n".join([*lines, "no portal in reach has a floor to walk, step, or hop into it from"])
    report = run(ctx, start.spawn_for(stands[0].feet), actions)
    begun = len(start.begin(stands[0].feet))
    labels = {(level, cell): label for label, level, cells, _ in labelled_platforms(MapIndex(ctx)) for cell in cells}
    steps = iter(report["steps"])
    blocked = entered = 0
    reached = []
    for entry, exit, name, length in attempts:
        block = [next(steps) for _ in range(length)]
        if block[begun - 1]["result"]["status"] not in ("reset", "teleported"):
            blocked += 1
            continue
        full = (block[begun - 1]["state"]["player"] or {}).get("health")
        outcome = Outcome.read(block[begun:])
        if outcome.refused or not outcome.crossings:
            continue
        entered += 1
        if target.holds(ctx, outcome.player):
            reached.append(f"  {name} at {entry.label}, out of {exit.label}: {_end(ctx, labels, outcome, full)}")
    tried = len(attempts) - blocked
    lines.append(f"{tried} entries tried, {entered} pass through a portal, {blocked} start inside geometry")
    if reached:
        lines.append(f"SHORTCUT: {len(reached)} reach the goal without building speed")
        lines.extend(reached)
    else:
        lines.append("none reaches the goal")
    return "\n".join(lines)

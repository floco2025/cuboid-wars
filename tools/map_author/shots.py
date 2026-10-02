"""What portal shots from a standing point do, by the game's own placement rule."""

from __future__ import annotations

from dataclasses import dataclass
from math import dist, floor, hypot

from map_editor.geometry import zone_rect
from map_editor.portal_surfaces import PortalSurface

from . import game
from .context import MapContext
from .describe import wall_groups
from .edges import SIDE_OF_FACE, STEP, surface_of_side, surface_spec
from .index import Cell, MapIndex
from .measure import parse_point, parse_surface

# A shot whose ray lands farther than this from its target met something else first.
ON_TARGET = 0.3
# A wall is aimed at this far above where a portal's rim would touch its
# base: resting exactly on the floor, the placement rule nudges the aperture up.
WALL_CLEARANCE = 0.02
# The cosine between a hit's normal and its target's from which they are one surface.
FACING = 0.9
Point = tuple[float, float, float]


# A point on a surface to aim at, in world metres, under the name the tools
# give it, and the way that surface faces.
@dataclass(frozen=True)
class Target:
    spec: str
    point: Point
    normal: Point


@dataclass(frozen=True)
class Stand:
    label: str
    feet: Point
    eye: Point


# The game's answer for one shot: `status` is placed, no_fit,
# incompatible_material, or no_surface; `hit` is where the ray landed and
# `portal` the aperture's centre, normal, and yaw.
@dataclass(frozen=True)
class Shot:
    stand: Stand
    target: Target
    status: str
    hit: Point | None = None
    hit_normal: Point | None = None
    portal: dict | None = None

    # A floor's underside lies a slab's thickness from its top, so the
    # surface a shot landed on is told by its facing as well.
    @property
    def on_target(self) -> bool:
        return (
            self.hit is not None
            and dist(self.hit, self.target.point) <= ON_TARGET
            and sum(a * b for a, b in zip(self.hit_normal, self.target.normal)) >= FACING
        )

    @property
    def opens(self) -> bool:
        return self.status == "placed" and self.on_target


# `L<level>:<x>,<z>` in cells: a body standing there on that level's floor.
def stand_at(ctx: MapContext, spec: str) -> Stand:
    level, gx, gz = parse_point(spec)
    x, z = ctx.frame.grid_to_world(gx, gz)
    y = ctx.frame.level_y(level)
    return Stand(f"L{level} ({gx:g}, {gz:g})", (x, y, z), (x, y + game.eye_height(ctx), z))


# Evenly spread points along `cells` cells, one per `step` cells that fit.
def _spread(cells: int, step: int) -> list[float]:
    count = max(cells // step, 1)
    return [(i + 0.5) * cells / count for i in range(count)]


def _wall_targets(ctx: MapContext, index: MapIndex) -> list[Target]:
    targets = []
    for level in range(index.count):
        for group in wall_groups(index, level):
            side = SIDE_OF_FACE[group[0].face]
            for along in _spread(len(group), ctx.footprint.across):
                # A whole number is the point two cells share: the end of the earlier one.
                at = min(floor(along), len(group) - 1) if along % 1 else int(along) - 1
                targets.append(_surface_target(ctx, surface_of_side(level, *group[at].cell, side, along - at)))
    return targets


def _surface_target(ctx: MapContext, surface: PortalSurface) -> Target:
    frame = surface.frame(ctx.settings)
    x, y, z = frame.center
    wx, wz = ctx.frame.metres_to_world(x, z)
    point = wx, y + (surface.face != "floor") * WALL_CLEARANCE, wz
    return Target(surface_spec(surface), point, tuple(map(float, frame.normal)))


# One point per portal that fits side by side on a region of cells, on the cells themselves.
def _region_points(ctx: MapContext, cells: frozenset[Cell] | set[Cell]) -> list[tuple[float, float]]:
    cols = [c for c, _ in cells]
    rows = [r for _, r in cells]
    c0, r0, c1, r1 = min(cols), min(rows), max(cols) + 1, max(rows) + 1
    points = [
        (c0 + dx, r0 + dz)
        for dz in _spread(r1 - r0, ctx.footprint.along)
        for dx in _spread(c1 - c0, ctx.footprint.along)
        if (floor(c0 + dx - 1e-6), floor(r0 + dz - 1e-6)) in cells
    ]
    if not points:
        col, row = min(cells, key=lambda cell: (cell[1], cell[0]))
        points = [(col + 0.5, row + 0.5)]
    return points


def _slab_targets(ctx: MapContext, index: MapIndex) -> list[Target]:
    targets = []
    frame = ctx.frame
    for level in range(index.count):
        for surface in index.floor_surfaces(level):
            for gx, gz in _region_points(ctx, surface.cells):
                x, z = frame.grid_to_world(gx, gz)
                targets.append(Target(f"floor:L{level}:{gx:g},{gz:g}", (x, frame.level_y(level), z), (0.0, 1.0, 0.0)))
        slabs = {**index.blocked[level], **index.floors[level]}
        under = {cell for cell, record in slabs.items() if index.portalable(record, "bottom")}
        for cells in index.components(level, under) if level else []:
            for gx, gz in _region_points(ctx, cells):
                x, z = frame.grid_to_world(gx, gz)
                y = frame.level_y(level) - frame.floor_thickness
                targets.append(Target(f"ceiling:L{level}:{gx:g},{gz:g}", (x, y, z), (0.0, -1.0, 0.0)))
    return targets


# The middle of a ramp's incline, which faces up and back down the slope.
def _ramp_targets(ctx: MapContext, index: MapIndex) -> list[Target]:
    targets = []
    for ramp in ctx.data["ramps"]:
        if not index.portalable(ramp, "top"):
            continue
        c0, r0, c1, r1 = zone_rect(ramp)
        gx, gz = (c0 + c1) / 2, (r0 + r1) / 2
        x, z = ctx.frame.grid_to_world(gx, gz)
        rise = ramp["levels"] * ctx.frame.level_height
        dx, dz = STEP[ramp["direction"]]
        run = ((c1 - c0) * abs(dx) + (r1 - r0) * abs(dz)) * ctx.frame.cell
        length = hypot(rise, run)
        normal = -dx * rise / length, run / length, -dz * rise / length
        point = x, ctx.frame.level_y(ramp["lower_level"]) + rise / 2, z
        targets.append(Target(f"ramp:L{ramp['lower_level']}:{gx:g},{gz:g}", point, normal))
    return targets


# Every surface whose material takes a portal, sampled where portals fit side
# by side: walls at the height a portal rests on their base, floors,
# ceilings, and ramps. Whether a portal opens there is the game's to say.
def candidates(ctx: MapContext) -> list[Target]:
    index = MapIndex(ctx)
    return [*_wall_targets(ctx, index), *_slab_targets(ctx, index), *_ramp_targets(ctx, index)]


# The targets named by `specs`: a wall or floor spec anywhere, or a candidate's own name.
def named_targets(ctx: MapContext, specs, known: list[Target]) -> list[Target]:
    by_spec = {target.spec: target for target in known}
    targets = []
    for spec in specs:
        if spec in by_spec:
            targets.append(by_spec[spec])
        elif spec.startswith(("wall:", "floor:")):
            targets.append(_surface_target(ctx, parse_surface(spec)))
        else:
            raise ValueError(f"{spec!r} is not a wall or floor spec or a surface `shots` lists")
    return targets


def probe_action(stand: Stand, targets: list[Target]) -> dict:
    return {"action": "probe", "eye": list(stand.eye), "targets": [list(target.point) for target in targets]}


def read_probe(stand: Stand, targets: list[Target], result: dict) -> list[Shot]:
    shots = []
    for target, shot in zip(targets, result["shots"], strict=True):
        hit = shot.get("hit")
        shots.append(
            Shot(
                stand,
                target,
                shot["status"],
                tuple(hit["position"]) if hit else None,
                tuple(hit["normal"]) if hit else None,
                shot.get("portal"),
            )
        )
    return shots


def probe(ctx: MapContext, stands: list[Stand], targets: list[Target], run=game.run) -> list[list[Shot]]:
    report = run(ctx, stands[0].feet, [probe_action(stand, targets) for stand in stands])
    return [read_probe(stand, targets, step["result"]) for stand, step in zip(stands, report["steps"], strict=True)]


def _point(values) -> str:
    return "(" + ", ".join(f"{value:.2f}" for value in values) + ")"


def _normal(values) -> str:
    return "(" + ", ".join(f"{value:.2g}" if abs(value) > 1e-3 else "0" for value in values) + ")"


def verdict(shot: Shot) -> str:
    if shot.status == "no_surface":
        return "nothing within range"
    if not shot.on_target:
        landing = f"the shot lands at {_point(shot.hit)} on a surface facing {_normal(shot.hit_normal)}"
        if shot.status == "placed":
            return f"BLOCKED: {landing} and opens a portal at {_point(shot.portal['position'])}"
        return f"BLOCKED: {landing} {'where no portal fits' if shot.status == 'no_fit' else 'that takes no portal'}"
    if shot.status == "placed":
        nudge = dist(shot.portal["position"], shot.hit)
        moved = f", nudged {nudge:.2f} m" if nudge > 0.01 else ""
        return f"opens at {_point(shot.portal['position'])} normal {_normal(shot.portal['normal'])}{moved}"
    if shot.status == "no_fit":
        return "no fit: nothing within the nudge backs the whole aperture with a clear front"
    return "fizzles: the aperture would cover a surface that takes no portal"


def shots(ctx: MapContext, stand_spec: str, specs=(), run=game.run) -> str:
    stand = stand_at(ctx, stand_spec)
    known = candidates(ctx)
    targets = named_targets(ctx, specs, known) if specs else known
    if not targets:
        return f"shots from {stand.label}: the map has no portalable surface"
    (results,) = probe(ctx, [stand], targets, run)
    width = max(len(target.spec) for target in targets)
    lines = [f"shots from {stand.label}, eye world {_point(stand.eye)}: {len(targets)} targets"]
    lines.extend(f"  {shot.target.spec:<{width}}  {verdict(shot)}" for shot in results)
    lines.append(f"{sum(shot.opens for shot in results)} of {len(results)} open a portal where aimed")
    return "\n".join(lines)

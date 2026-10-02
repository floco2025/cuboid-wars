"""Flights through the game's own step, funnel, and hop, printed per level."""

from __future__ import annotations

import re

from map_editor.floor_footprints import FloorFootprints
from map_editor.jump_path import AFTER_EXIT, Flight, Takeoff, build_request, entry_outcome, jump_preview
from map_editor.portal_surfaces import PortalSurface, PortalSurfaces, portals_overlap

from .context import MapContext
from .edges import along_of_surface, cell_side_of_surface, surface_of_side, surface_spec
from .index import MapIndex

SCENARIOS = ("normal", "speed", "low gravity", "speed+low gravity")
TAKEOFF_RE = re.compile(r"^L(\d+):(-?\d+),(-?\d+):([NSEW])(?::([0-9.]+))?$")
WALL_RE = re.compile(r"^wall:L(\d+):(-?\d+),(-?\d+):([NSEW])(?::([0-9.]+))?$")
FLOOR_RE = re.compile(r"^floor:L(\d+):(-?[0-9.]+),(-?[0-9.]+)$")
POINT_RE = re.compile(r"^L(\d+):(-?[0-9.]+),(-?[0-9.]+)$")


# `L<level>:<col>,<row>:<side>[:<along>]`: off that side of the cell, `along` the edge (0 to 1).
def parse_takeoff(spec: str) -> Takeoff:
    match = TAKEOFF_RE.match(spec)
    if not match:
        raise ValueError(f"takeoff {spec!r} is not L<level>:<col>,<row>:<N|S|E|W>[:<along>]")
    level, col, row, side, along = match.groups()
    return Takeoff(int(level), int(col), int(row), side, float(along) if along else 0.5)


# `wall:L<level>:<col>,<row>:<side>[:<along>]` is the wall on that side of the
# cell, its portal facing the cell and centred `along` the edge (0 to 1, the
# middle without one); `floor:L<level>:<x>,<z>` is a floor point in cells.
def parse_surface(spec: str) -> PortalSurface:
    match = WALL_RE.match(spec)
    if match:
        level, col, row, side, along = match.groups()
        return surface_of_side(int(level), int(col), int(row), side, float(along) if along else 0.5)
    match = FLOOR_RE.match(spec)
    if match:
        level, x, z = match.groups()
        return PortalSurface.floor_at(int(level), float(x), float(z))
    raise ValueError(f"surface {spec!r} is not wall:L<level>:<col>,<row>:<side>[:<along>] or floor:L<level>:<x>,<z>")


def parse_point(spec: str) -> tuple[int, float, float]:
    match = POINT_RE.match(spec)
    if not match:
        raise ValueError(f"point {spec!r} is not L<level>:<x>,<z> in cells")
    level, x, z = match.groups()
    return int(level), float(x), float(z)


def footprints(ctx: MapContext) -> FloorFootprints:
    return FloorFootprints(ctx.data, ctx.frame.cell, ctx.frame.wall_thickness)


def _check_takeoff(ctx: MapContext, takeoff: Takeoff, prints: FloorFootprints) -> None:
    if not 0 <= takeoff.level < ctx.level_count:
        raise ValueError(f"level {takeoff.level} is outside the map's {ctx.level_count} levels")
    if (takeoff.col, takeoff.row) not in prints.cells[takeoff.level]:
        raise ValueError(f"cell ({takeoff.col}, {takeoff.row}) on L{takeoff.level} has no floor to take off from")


def jump(
    ctx: MapContext,
    takeoff: Takeoff,
    *,
    walk: bool = False,
    late: float = 0.0,
    air_control: bool = False,
    entry: PortalSurface | None = None,
    exit: PortalSurface | None = None,
    shooter: tuple[float, float] | None = None,
    into: bool = False,
) -> str:
    prints = footprints(ctx)
    _check_takeoff(ctx, takeoff, prints)
    shooter = shooter or takeoff.grid_point
    lines = []
    if into:
        if (
            entry is None
            or entry.face == "floor"
            or cell_side_of_surface(entry) != (takeoff.col, takeoff.row, takeoff.side)
            or along_of_surface(entry) != takeoff.along
        ):
            raise ValueError(
                "a walk into a portal needs the entry to be the wall on the takeoff's side of its cell, "
                "at the same point along it"
            )
        walk = True
    if entry is not None:
        entry = entry.placed_from(shooter)
        exit = exit.placed_from(shooter) if exit is not None else None
        for surface in (entry, exit):
            if surface is not None:
                status = PortalSurfaces(ctx.data, ctx.textures).status(surface)
                if not status.available:
                    raise ValueError(f"{surface_spec(surface)}: {status.reason}")
        if exit is not None and portals_overlap(entry, exit, ctx.settings):
            raise ValueError("the two portals overlap")
    request = build_request(
        ctx.settings,
        prints,
        takeoff,
        levels=ctx.level_count,
        jumping=not walk,
        margin=late,
        air_control=air_control,
        shooter=shooter,
        entry=entry,
        exit=exit,
    )
    if into:
        # Start a body's radius from the wall face, moving at full speed, so
        # the first tick crosses the plane as a run into the wall does.
        normal = entry.frame(ctx.settings).normal
        gx, gz = takeoff.grid_point
        back = ctx.frame.wall_thickness / 2 + ctx.settings.physics["body"]["diameter"] / 2
        request["takeoff"]["point"] = [
            gx * ctx.frame.cell + normal[0] * back,
            ctx.settings.floor_height(takeoff.level),
            gz * ctx.frame.cell + normal[2] * back,
        ]
    preview = jump_preview(ctx.settings, request)
    point = request["takeoff"]["point"]
    wx, wz = ctx.frame.metres_to_world(point[0], point[2])
    mode = "walk into the wall" if into else "walk-off" if walk else f"jump{f' {late:+.2f} s' if late else ''}"
    lines.append(
        f"takeoff L{takeoff.level} cell ({takeoff.col}, {takeoff.row}) side {takeoff.side} along {takeoff.along:g}: "
        f"{mode} heading {takeoff.side}, world ({wx:.2f}, {point[1]:.2f}, {wz:.2f})"
        + (" with air control" if air_control else "")
    )
    if entry is not None:
        lines.append(
            f"portal 1 {surface_spec(entry)}" + (f", portal 2 {surface_spec(exit)}" if exit else ", no portal 2")
        )
    for name, flight in zip(SCENARIOS, preview.flights):
        flight_lines, lowest = _flight_lines(ctx, prints, name, flight, preview.reach, entry is not None)
        lines.extend(flight_lines)
        lines.extend(_range_lines(ctx, flight, point, takeoff.direction, lowest))
    return "\n".join(lines)


# How far holding a direction carries, per level: the preview's steering hull
# measured along the heading and across it.
def _range_lines(ctx: MapContext, flight: Flight, point, direction, lowest: int) -> list[str]:
    lines = []
    for level, polygons in sorted({**flight.range, **flight.exit_range}.items(), reverse=True):
        if level < lowest:
            break
        along, across = 0.0, 0.0
        for polygon in polygons:
            for x, z in polygon:
                dx, dz = x - point[0], z - point[2]
                along = max(along, dx * direction[0] + dz * direction[1])
                across = max(across, abs(dx * direction[1] - dz * direction[0]))
        lines.append(
            f"  holding a direction: on L{level} reaches up to {along:.1f} m ({along / ctx.frame.cell:.1f} cells) "
            f"along the heading and {across:.1f} m to either side"
        )
    return lines


def _flight_lines(ctx: MapContext, prints: FloorFootprints, name: str, flight: Flight, reach: float, portals: bool):
    outcome = entry_outcome(flight, prints, reach) if portals else None
    end = {"below": "falls below every floor", "time_cap": "still flying at the time cap"}.get(flight.end, flight.end)
    head = f"{name}: {end}"
    if portals:
        head += f", portal 1 {outcome}"
        if flight.hop_time is not None:
            head += f" at {flight.hop_time:.2f} s"
    lines = [head]
    for crossing in flight.crossings:
        supported = prints.floor_under(crossing.level, *crossing.point, reach)
        cell = prints.supporting_cell(crossing.level, *crossing.point, reach)
        x, z = ctx.frame.metres_to_world(*crossing.point)
        gx, gz = crossing.point[0] / ctx.frame.cell, crossing.point[1] / ctx.frame.cell
        damage = "fatal" if crossing.damage >= 1 else f"{crossing.damage * 100:.0f}% damage"
        phase = " after exit" if crossing.phase == AFTER_EXIT else ""
        verdict = f"LANDS on cell {cell}" if supported else "no floor there"
        lines.append(
            f"  L{crossing.level}{phase} at {crossing.time:.2f} s: cell ({gx:.1f}, {gz:.1f}) "
            f"world ({x:.2f}, {ctx.frame.level_y(crossing.level):.2f}, {z:.2f})  {damage}  {verdict}"
        )
        # The flight ends on the first floor; past a fatal speed the lower
        # levels say nothing new, unless a portal still catches the body.
        past_portal = flight.hop_time is None or crossing.time > flight.hop_time
        if supported or (crossing.damage >= 1 and past_portal):
            return lines, crossing.level
    return lines, 0


def surface(ctx: MapContext, spec: PortalSurface) -> str:
    index = MapIndex(ctx)
    status = PortalSurfaces(ctx.data, ctx.textures).status(spec)
    frame = spec.frame(ctx.settings)
    x, z = ctx.frame.metres_to_world(frame.center[0], frame.center[2])
    lines = [
        f"{surface_spec(spec)}  centre world ({x:.2f}, {frame.center[1]:.2f}, {z:.2f})  normal {tuple(frame.normal)}"
    ]
    if not status.available:
        lines.append(f"not portalable: {status.reason}")
        return "\n".join(lines)
    if spec.face == "floor":
        cell = spec.col, spec.row
        pads = [s for s in index.floor_surfaces(spec.level) if cell in s.cells]
        if not pads:
            lines.append("no portalable floor cell there")
        elif pads[0].ready:
            c0, r0, c1, r1 = pads[0].bounds
            lines.append(f"ready: pad cols {c0}..{c1} rows {r0}..{r1}, long axis {' or '.join(pads[0].axes)}")
        else:
            lines.append(f"not ready: {pads[0].reason}")
    else:
        key = ("h" if spec.face in ("north", "south") else "v", spec.col, spec.row)
        reason = index.wall_reason(spec.level, key, spec.face)
        if reason is None:
            lines.append("no wall on that edge")
        elif reason:
            lines.append(f"not ready: {reason}")
        else:
            storeys = ctx.footprint.storeys
            lines.append(f"ready: {storeys} portalable storey{'' if storeys == 1 else 's'} with a clear front")
    return "\n".join(lines)


# Reach on open ground, from the game's physics alone: how far a body flies
# before it comes down one, two, or three storeys below, level with the
# takeoff, or one above.
def ranges(ctx: MapContext) -> str:
    settings = ctx.settings
    down, up = 3, 1
    heights = [settings.floor_height(i) for i in range(down + up + 1)]
    lines = [
        f"reach in metres (cells of {ctx.frame.cell:g} m) from a full-speed takeoff, input released or held "
        f"forward, by storeys down (-) or up (+) from the takeoff"
    ]
    header = "mode            scenario            " + "".join(f"{dy:>+9d}" for dy in range(up, -down - 1, -1))
    lines.append(header)
    for walk, held in ((False, False), (False, True), (True, False), (True, True)):
        request = {
            "takeoff": {
                "point": [0.0, heights[down], 0.0],
                "direction": [1.0, 0.0],
                "jumping": not walk,
                "margin": 0.0,
            },
            "heights": heights,
            "air_control": held,
            "shooter": [0.0, 0.0],
            "portals": None,
        }
        preview = jump_preview(settings, request)
        mode = ("walk-off" if walk else "jump") + (", held" if held else "")
        for name, flight in zip(SCENARIOS, preview.flights):
            cells = []
            for dy in range(up, -down - 1, -1):
                level = down + dy
                if held:
                    reach = max((x for polygon in flight.range.get(level, ()) for x, _ in polygon), default=None)
                else:
                    crossing = next((c for c in flight.crossings if c.level == level), None)
                    reach = None if crossing is None else crossing.point[0]
                cells.append("        -" if reach is None else f"{reach:>9.1f}")
            lines.append(f"{mode:<15} {name:<19} " + "".join(cells))
    lines.append(
        f"apex {settings.physics['player']['jump_speed'] ** 2 / (2 * settings.physics['gravity']):.2f} m normal, "
        f"{settings.physics['player']['jump_speed'] ** 2 / (2 * settings.physics['low_gravity']):.2f} m low gravity; "
        f"one storey is {ctx.frame.level_height:g} m"
    )
    return "\n".join(lines)


def where(ctx: MapContext, grid: str | None = None, world: str | None = None) -> str:
    frame = ctx.frame
    if grid is not None:
        level, gx, gz = parse_point(grid)
        x, z = frame.grid_to_world(gx, gz)
        return f"L{level} cell ({gx:g}, {gz:g}) is world ({x:.2f}, {frame.level_y(level):.2f}, {z:.2f})"
    x, y, z = (float(v) for v in world.split(","))
    gx, gz = frame.world_to_grid(x, z)
    return f"world ({x:g}, {y:g}, {z:g}) is {frame.describe_y(y)} cell ({gx:.2f}, {gz:.2f})"

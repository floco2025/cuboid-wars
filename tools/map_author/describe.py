"""Text views of a map: a per-level plan and a structural summary."""

from __future__ import annotations

import string

from map_editor.geometry import ramp_slope, zone_rect

from .context import MapContext
from .edges import SIDE_OF_FACE, cells_of_edge, surface_of_side, surface_spec
from .index import FloorSurface, MapIndex, WallSurface

ITEM_GLYPHS = {
    "speed": "s",
    "low_gravity": "g",
    "equipment_eraser": "e",
    "key": "k",
    "portal_gun": "p",
    "health_potion": "h",
    "missile_pack": "m",
    "gold": "$",
    "single_shot": "w",
    "multi_shot": "w",
}
RAMP_GLYPHS = {"N": "^", "S": "v", "W": "<", "E": ">"}
# Farther than this, two platforms are not neighbours worth a gap line.
GAP_LIMIT = 12
LEGEND = """legend: . floor  o portalable floor  O portal-ready floor  # blocked floor  _ light bridge  ~ terrain
        ^ v < > ramp (rises toward)  @ plate  0-9 checkpoint  (space) void
        items: s speed  g low gravity  e eraser  k key  p portal gun  h health  m missiles  $ gold  w weapon
        edges: | - wall  : ~ portalable wall  I = portal-ready wall  % barrier  x eraser  H ladder"""


def plan(ctx: MapContext, level: int | None = None, *, legend: bool = True) -> str:
    index = MapIndex(ctx)
    levels = range(index.count) if level is None else [level]
    blocks = [_level_plan(ctx, index, i) for i in levels]
    if legend:
        blocks.append(LEGEND)
    return "\n".join(blocks)


def _level_plan(ctx: MapContext, index: MapIndex, level: int) -> str:
    name = ctx.data["levels"][level].get("name") or f"Level {level}"
    header = f'## L{level} "{name}"  y={ctx.frame.level_y(level):.2f}'
    extent = index.extent(level)
    if extent is None:
        return f"{header}  (empty)"
    c0, r0, c1, r1 = extent
    cols = range(c0, c1)
    ready_walls = {(s.key, s.face) for s in index.wall_surfaces(level) if s.ready}
    portalable_walls = {s.key for s in index.wall_surfaces(level)}
    ready_floor = {cell for s in index.floor_surfaces(level) if s.ready for cell in s.cells}

    def vertical(col, row):
        key = ("v", col, row)
        if key in index.ladders[level]:
            return "H"
        if key in index.erasers[level]:
            return "x"
        if key in index.barriers[level]:
            return "%"
        if key in index.walls[level]:
            if (key, "west") in ready_walls or (key, "east") in ready_walls:
                return "I"
            return ":" if key in portalable_walls else "|"
        return " "

    def horizontal(col, row):
        key = ("h", col, row)
        if key in index.ladders[level]:
            return "H"
        if key in index.erasers[level]:
            return "x"
        if key in index.barriers[level]:
            return "%"
        if key in index.walls[level]:
            if (key, "north") in ready_walls or (key, "south") in ready_walls:
                return "="
            return "~" if key in portalable_walls else "-"
        return " "

    def glyph(cell):
        if cell in index.plates[level]:
            return "@"
        if cell in index.items[level]:
            return ITEM_GLYPHS.get(index.items[level][cell]["type"], "*")
        if cell in index.ramp_cells[level]:
            return RAMP_GLYPHS[index.ramp_cells[level][cell]["direction"]]
        number = index.checkpoint_at(level, cell)
        if number is not None:
            return str(number % 10)
        if cell in index.bridges[level]:
            return "_"
        if cell in index.terrain[level]:
            return "~"
        if cell in index.blocked[level]:
            return "#"
        if cell in ready_floor:
            return "O"
        if cell in index.floors[level]:
            return "o" if index.portalable_top(level, cell) else "."
        return " "

    lines = [f"{header}  cols {c0}..{c1 - 1}  rows {r0}..{r1 - 1}"]
    if c1 > 10:
        lines.append("     " + " ".join(str(col // 10) if col >= 10 else " " for col in cols))
    lines.append("     " + " ".join(str(col % 10) for col in cols))
    for row in range(r0, r1 + 1):
        edges = "".join(horizontal(col, row) + " " for col in cols).rstrip()
        if edges:
            lines.append("     " + edges)
        if row < r1:
            cells = "".join(glyph((col, row)) + vertical(col + 1, row) for col in cols)
            lines.append(f"{row:3d} " + vertical(c0, row) + cells.rstrip())
    return "\n".join(lines)


def summary(ctx: MapContext) -> str:
    index = MapIndex(ctx)
    frame, need = ctx.frame, ctx.footprint
    lines = [
        f"map {ctx.name}  {frame.cols}x{frame.rows} cells  cell {frame.cell:g} m  level {frame.level_height:g} m  "
        f"world x {-frame.width / 2:g}..{frame.width / 2:g}  z {-frame.depth / 2:g}..{frame.depth / 2:g}",
        f"a portal needs a wall {_count(need.across, 'cell')} wide and {_count(need.storeys, 'section')} tall, "
        f"or a floor of {need.along}x{need.across} cells",
    ]
    platforms = labelled_platforms(index)
    seen = set()
    for level in range(index.count):
        own = [p for p in platforms if p[1] == level]
        bridges = index.components(level, set(index.bridges[level]))
        if not own and not bridges:
            continue
        name = ctx.data["levels"][level].get("name") or f"Level {level}"
        lines.append(f'L{level} "{name}" y={frame.level_y(level):.2f}')
        for label, _, cells, bounds in own:
            lines.append(f"  {label:<7}{_platform_line(ctx, index, level, cells, bounds)}")
        for cells in bridges:
            c0, r0, c1, r1 = _bounds(cells)
            field = index.bridges[level][min(cells)]
            lines.append(f"  bridge {field!r} cols {c0}..{c1} rows {r0}..{r1} ({c1 - c0}x{r1 - r0})")
        gaps = [part for p in own for part in _gap_parts(frame, p, platforms, seen)]
        if gaps:
            lines.append("  gaps: " + " | ".join(gaps))
    lines.extend(_surface_lines(ctx, index))
    lines.extend(_structure_lines(ctx, index, platforms))
    return "\n".join(lines)


# Every platform as the summary names it: label, level, cells, bounds.
def labelled_platforms(index: MapIndex) -> list[tuple[str, int, set, tuple[int, int, int, int]]]:
    return [
        (f"L{level}.{letter}", level, cells, _bounds(cells))
        for level in range(index.count)
        for letter, cells in zip(_letters(), index.platforms(level))
    ]


def _count(number: int, noun: str) -> str:
    return f"{number} {noun}{'' if number == 1 else 's'}"


def _letters():
    for first in ("", *string.ascii_lowercase):
        for second in string.ascii_lowercase:
            yield first + second


def _bounds(cells) -> tuple[int, int, int, int]:
    cols = [c for c, _ in cells]
    rows = [r for _, r in cells]
    return min(cols), min(rows), max(cols) + 1, max(rows) + 1


def _platform_line(ctx: MapContext, index: MapIndex, level: int, cells, bounds) -> str:
    c0, r0, c1, r1 = bounds
    x0, z0, x1, z1 = ctx.frame.world_bounds(c0, r0, c1, r1)
    kinds = []
    if any(c in index.floors[level] for c in cells):
        kinds.append("floor")
    if any(c in index.blocked[level] for c in cells):
        kinds.append("blocked")
    if any(c in index.terrain[level] for c in cells):
        kinds.append("terrain")
    shape = f"cols {c0}..{c1} rows {r0}..{r1} ({c1 - c0}x{r1 - r0}"
    shape += ")" if len(cells) == (c1 - c0) * (r1 - r0) else f", {len(cells)} cells)"
    holds = []
    for zone in index.checkpoints[level]:
        zc0, zr0, zc1, zr1 = zone_rect(zone)
        if (zc0, zr0) in cells:
            holds.append(f"cp{zone['number']}")
    for cell, switch in sorted(index.plates[level].items()):
        if cell in cells:
            holds.append(f"plate {switch!r} {cell}")
    for cell, item in sorted(index.items[level].items()):
        if cell in cells:
            label = item["type"] + (f"[{item['field']}]" if "field" in item else "")
            holds.append(f"{label} {cell}")
    text = f"{shape} {'+'.join(kinds)}  world x {x0:g}..{x1:g} z {z0:g}..{z1:g}"
    return text + (f"  holds: {', '.join(holds)}" if holds else "")


# The nearest platform in each direction whose span overlaps, any level; a pair is told once.
def _gap_parts(frame, platform, platforms, seen: set) -> list[str]:
    label, level, _, (c0, r0, c1, r1) = platform
    parts = []
    for side, distance, overlaps in (
        ("E", lambda b: b[0] - c1, lambda b: b[1] < r1 and r0 < b[3]),
        ("W", lambda b: c0 - b[2], lambda b: b[1] < r1 and r0 < b[3]),
        ("S", lambda b: b[1] - r1, lambda b: b[0] < c1 and c0 < b[2]),
        ("N", lambda b: r0 - b[3], lambda b: b[0] < c1 and c0 < b[2]),
    ):
        best = None
        for other in platforms:
            if other is platform or not overlaps(other[3]):
                continue
            cells = distance(other[3])
            if 0 <= cells <= GAP_LIMIT and (best is None or cells < best[0]):
                best = cells, other
        if best is None:
            continue
        cells, other = best
        if frozenset((label, other[0])) in seen:
            continue
        seen.add(frozenset((label, other[0])))
        dy = other[1] - level
        metres = max(cells * frame.cell - frame.wall_thickness, 0.0)
        parts.append(
            f"{label} -{side}-> {other[0]} {cells} cells ({metres:.1f} m edge to edge) dy {dy:+d} ({dy * frame.level_height:+.1f} m)"
        )
    return parts


def _surface_lines(ctx: MapContext, index: MapIndex) -> list[str]:
    ready, blocked = [], []
    for level in range(index.count):
        for group in wall_groups(index, level):
            (ready if group[0].ready else blocked).append(_wall_line(ctx, group))
        for surface in index.floor_surfaces(level):
            (ready if surface.ready else blocked).append(_floor_line(ctx, surface))
    lines = ["portal-ready surfaces:" if ready else "portal-ready surfaces: none"]
    lines.extend(f"  {line}" for line in ready)
    if blocked:
        lines.append("portalable but not ready:")
        lines.extend(f"  {line}" for line in blocked)
    return lines


# Runs of unit edges along one line with the same face and verdict, as one
# surface; the upper sections of a ready stack are not reported on their own.
def wall_groups(index: MapIndex, level: int) -> list[list[WallSurface]]:
    groups = []
    surfaces = sorted(
        index.wall_surfaces(level),
        key=lambda s: (s.face, s.key[2], s.key[1]) if s.key[0] == "h" else (s.face, s.key[1], s.key[2]),
    )
    for surface in surfaces:
        axis, col, row = surface.key
        if any(
            s.ready and s.key == surface.key and s.face == surface.face
            for below in range(max(level - index.ctx.footprint.storeys + 1, 0), level)
            for s in index.wall_surfaces(below)
        ):
            continue
        last = groups[-1][-1] if groups else None
        if (
            last is not None
            and last.face == surface.face
            and last.reason == surface.reason
            and last.key[0] == axis
            and (
                (axis == "h" and last.key[2] == row and last.key[1] + 1 == col)
                or (axis == "v" and last.key[1] == col and last.key[2] + 1 == row)
            )
        ):
            groups[-1].append(surface)
        else:
            groups.append([surface])
    return groups


def _wall_line(ctx: MapContext, group: list[WallSurface]) -> str:
    first, last = group[0], group[-1]
    col, row = first.cell
    side = SIDE_OF_FACE[first.face]
    frames = [surface_of_side(s.level, *s.cell, side).frame(ctx.settings) for s in group]
    cx = sum(f.center[0] for f in frames) / len(frames)
    cz = sum(f.center[2] for f in frames) / len(frames)
    x, z = ctx.frame.metres_to_world(cx, cz)
    normal = tuple(int(n) for n in frames[0].normal)
    span, centre = "", ""
    if len(group) > 1:
        span = f" to ({last.cell[0]}, {last.cell[1]}), {len(group)} cells"
        # The spec of the run's middle: a cell's edge, or the point two cells share.
        middle = group[(len(group) - 1) // 2]
        along = 0.5 if len(group) % 2 else 1.0
        centre = surface_spec(surface_of_side(middle.level, *middle.cell, side, along)) + " "
    text = (
        f"wall  wall:L{first.level}:{col},{row}:{side}{span}  faces {first.face}  "
        f"centre {centre}world ({x:.2f}, {frames[0].center[1]:.2f}, {z:.2f}) normal {normal}"
    )
    return text if first.ready else f"{text}: {first.reason}"


def _floor_line(ctx: MapContext, surface: FloorSurface) -> str:
    c0, r0, c1, r1 = surface.bounds
    x, z = ctx.frame.grid_to_world((c0 + c1) / 2, (r0 + r1) / 2)
    axes = " and ".join(surface.axes) if surface.axes else "none"
    text = f"floor floor:L{surface.level}:{(c0 + c1) / 2:g},{(r0 + r1) / 2:g}  cols {c0}..{c1} rows {r0}..{r1}  long axis {axes}  centre world ({x:.2f}, {ctx.frame.level_y(surface.level):.2f}, {z:.2f})"
    return text if surface.ready else f"{text}: {surface.reason}"


def _actor_line(zone: dict) -> str:
    c0, r0, c1, r1 = zone_rect(zone)
    top = zone["level"] + zone.get("levels", 1) - 1
    levels = f"L{zone['level']}" + (f"..L{top}" if top > zone["level"] else "")
    respawn = zone.get("respawn_secs")
    parts = [
        f"actors {zone['kind']} x{'/'.join(str(n) for n in zone['count'])} {levels} cols {c0}..{c1} rows {r0}..{r1}",
        "no respawn" if respawn is None else f"respawn {respawn:g} s",
    ]
    if zone.get("roam_distance"):
        parts.append(f"roams {zone['roam_distance']:g} m")
    if zone.get("switch"):
        parts.append(f"switch {zone['switch']!r}")
    if not zone.get("initially_on", True):
        parts.append("initially off")
    if zone.get("until_checkpoint") is not None:
        parts.append(
            f"until cp{zone['until_checkpoint']}" + (f" ({zone['on_checkpoint']})" if zone.get("on_checkpoint") else "")
        )
    return ", ".join(parts)


# Consecutive unit edges on one line with the same value, as one run.
def _edge_runs(edges) -> list[tuple]:
    runs = []
    for key, value in sorted(
        edges,
        key=lambda e: (
            (e[0][0], e[1] or "", e[0][2], e[0][1]) if e[0][0] == "h" else (e[0][0], e[1] or "", e[0][1], e[0][2])
        ),
    ):
        axis, col, row = key
        last = runs[-1] if runs else None
        if (
            last is not None
            and last[0] == value
            and last[1][-1][0] == axis
            and (
                (axis == "h" and last[1][-1][2] == row and last[1][-1][1] + 1 == col)
                or (axis == "v" and last[1][-1][1] == col and last[1][-1][2] + 1 == row)
            )
        ):
            last[1].append(key)
        else:
            runs.append((value, [key]))
    return runs


def _run_text(run: list) -> str:
    first, last = run[0], run[-1]
    a, b = cells_of_edge(first)
    if len(run) == 1:
        return f"between cells {a} and {b}"
    c, d = cells_of_edge(last)
    return f"between cells {a}..{c} and {b}..{d} ({len(run)} edges)"


def _structure_lines(ctx: MapContext, index: MapIndex, platforms) -> list[str]:
    lines = []
    data = ctx.data
    for ramp in data["ramps"]:
        c0, r0, c1, r1 = zone_rect(ramp)
        slope = ramp_slope(ramp, ctx.frame.cell, ctx.frame.level_height) or {}
        verdict = "climbable" if slope.get("climbable") else "too steep"
        lines.append(
            f"ramp L{ramp['lower_level']}->L{ramp['lower_level'] + ramp['levels']} cols {c0}..{c1} rows {r0}..{r1} "
            f"rising {ramp['direction']} ({ramp.get('shape', 'solid')})  {slope.get('degrees', 0):.1f}° {verdict}"
        )
    for ladder in data["ladders"]:
        top = ladder["lower_level"] + ladder["levels"]
        lines.append(
            f"ladder L{ladder['lower_level']}->L{top} on side {ladder['side']} of cell ({ladder['col']}, {ladder['row']}), "
            f"climbed from the cell across that edge"
        )
    for level in range(index.count):
        for field, run in _edge_runs(sorted(index.barriers[level].items())):
            lines.append(f"barrier {field!r} L{level} {_run_text(run)}")
        for _, run in _edge_runs(sorted((key, None) for key in index.erasers[level])):
            lines.append(f"eraser L{level} {_run_text(run)}")
    placed = {(level, cell) for _, level, cells, _ in platforms for cell in cells}
    for level in range(index.count):
        for cell, item in sorted(index.items[level].items()):
            if (level, cell) not in placed:
                lines.append(f"item {item['type']} L{level} {cell} floats with no floor under it")
    for level, records in enumerate(data["levels"]):
        kinds = sorted(light["kind"] for light in records.get("lights", []))
        if kinds:
            counts = ", ".join(f"{kinds.count(kind)} {kind}" for kind in dict.fromkeys(kinds))
            lines.append(f"lights L{level}: {counts}")
    lines.extend(_actor_line(zone) for zone in data.get("actor_spawn_zones", []))
    switches = data.get("switches") or []
    plates = {}
    for level in range(index.count):
        for cell, switch in index.plates[level].items():
            plates.setdefault(switch, []).append(f"L{level} {cell}")
    for switch in switches:
        operated = ", ".join(plates.get(switch["id"], [])) or "no plate"
        lines.append(
            f"switch {switch['id']!r}: {switch.get('activation', 'toggle')}, reset {switch.get('reset_on_player_death', 'solo')}, "
            f"held {switch.get('held', 'any')}; plates: {operated}"
        )
    for field in data.get("fields") or []:
        state = "on" if field.get("initially_on", True) else "off"
        switch = f" switch {field['switch']!r}" if field.get("switch") else " no switch"
        pieces = sum(1 for level in range(index.count) for f in index.barriers[level].values() if f == field["id"])
        bridges = sum(1 for level in range(index.count) for f in index.bridges[level].values() if f == field["id"])
        lines.append(
            f"field {field['id']!r}: initially {state},{switch}; {pieces} barrier edges, {bridges} bridge cells"
        )
    fireworks = data.get("fireworks")
    if fireworks:
        lines.append(f"fireworks on switch {fireworks['switch']!r}")
    return lines

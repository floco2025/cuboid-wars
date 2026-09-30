"""The Jump Path tool without Qt: a takeoff on a tile edge, the request
map_core flies, and its reply arranged for one viewed level."""

from dataclasses import dataclass
from math import floor, hypot

from .core import call
from .floor_footprints import FloorFootprints
from .geometry import cell_side_from_click
from .jump_settings import SCENARIO_BITS, JumpSettings
from .portal_surfaces import PortalSurface

# The heading out of a cell over each side, in (x, z).
SIDES = {"N": (0.0, -1.0), "S": (0.0, 1.0), "W": (-1.0, 0.0), "E": (1.0, 0.0)}
OPPOSITE = {"N": "S", "S": "N", "W": "E", "E": "W"}
BEFORE_ENTRY = "before_entry"
AFTER_EXIT = "after_exit"
# Reply coordinates are rounded to millimetres; points this close together are one, or on one line.
ON_LINE = 0.005


# The point on a cell's edge a flight leaves from, heading straight out over it.
@dataclass(frozen=True)
class Takeoff:
    level: int
    col: int
    row: int
    side: str
    # Along the edge, 0 to 1 toward the higher column or row.
    along: float

    @property
    def direction(self) -> tuple[float, float]:
        return SIDES[self.side]

    # On the grid line, in grid units.
    @property
    def grid_point(self) -> tuple[float, float]:
        if self.side in "NS":
            return self.col + self.along, self.row + (self.side == "S")
        return self.col + (self.side == "E"), self.row + self.along

    # On the slab's real edge, in metres.
    def point(self, settings: JumpSettings, footprints: FloorFootprints) -> tuple[float, float, float]:
        x, z = footprints.edge_point(self.level, self.col, self.row, self.side, self.along)
        return x, settings.floor_height(self.level), z


# A click, in grid units, takes off over the nearest edge of its cell at the
# point clicked. A click just past a ledge means that ledge: an empty cell
# whose nearest edge borders a slab takes off from the slab, toward the click.
def takeoff_from_click(level: int, x: float, z: float, cols: int, rows: int, slabs) -> Takeoff | None:
    col, row = floor(x), floor(z)
    if not (0 <= col < cols and 0 <= row < rows):
        return None
    side = cell_side_from_click(col, row, x, z)
    across = col + int(SIDES[side][0]), row + int(SIDES[side][1])
    if (col, row) not in slabs[level] and across in slabs[level]:
        (col, row), side = across, OPPOSITE[side]
    along = x - col if side in "NS" else z - row
    return Takeoff(level, col, row, side, min(max(along, 0.0), 1.0))


# `shooter` is where portal 1 is shot from, in grid units.
def build_request(
    settings: JumpSettings,
    footprints: FloorFootprints,
    takeoff: Takeoff,
    *,
    levels: int,
    jumping: bool,
    margin: float,
    air_control: bool,
    shooter: tuple[float, float],
    entry: PortalSurface | None = None,
    exit: PortalSurface | None = None,
) -> dict:
    return {
        "takeoff": {
            "point": list(takeoff.point(settings, footprints)),
            "direction": list(takeoff.direction),
            "jumping": jumping,
            "margin": margin if jumping else 0.0,
        },
        "heights": [settings.floor_height(level) for level in range(levels)],
        "air_control": air_control,
        "shooter": [shooter[0] * settings.cell_size, shooter[1] * settings.cell_size],
        "portals": None
        if entry is None
        else {"entry": entry.spec(settings), "exit": None if exit is None else exit.spec(settings)},
    }


@dataclass(frozen=True)
class Crossing:
    level: int
    phase: str
    point: tuple[float, float]
    time: float
    # Fraction of full health.
    damage: float


# One scenario of a preview; positions are metres.
@dataclass(frozen=True)
class Flight:
    bit: int
    # Feet per tick; `path[hop]` is a portal entrance and the next point its exit.
    path: tuple[tuple[float, float, float], ...]
    hop: int | None
    hop_time: float | None
    end: str
    crossings: tuple[Crossing, ...]
    # How the flight met portal 1: direct, funnel, steered, missed, or reversed.
    entry: str | None
    # Outlines by level.
    capture: dict
    capture_steered: dict
    range: dict
    exit_range: dict

    @classmethod
    def parse(cls, bit: int, scenario: dict) -> "Flight":
        def polygon(points):
            return tuple(map(tuple, points))

        def pieces(levels):
            return {entry["level"]: tuple(polygon(piece["polygon"]) for piece in entry["pieces"]) for entry in levels}

        def hulls(levels):
            return {entry["level"]: (polygon(entry["polygon"]),) for entry in levels}

        return cls(
            bit,
            polygon(scenario["path"]),
            scenario["hop"],
            scenario["hop_time"],
            scenario["end"],
            tuple(
                Crossing(c["level"], c["phase"], tuple(c["point"]), c["time"], c["damage"])
                for c in sorted(scenario["crossings"], key=lambda crossing: crossing["time"])
            ),
            scenario["entry"],
            pieces(scenario["capture"]),
            pieces(scenario["capture_steered"]),
            hulls(scenario["range"]),
            hulls(scenario["exit_range"]),
        )


# The flights of one request, one per power-up combination, and how far
# past a slab's edge the body still stands: the flights leave that far beyond
# the takeoff edge, and a landing counts that close to a slab.
@dataclass(frozen=True)
class Preview:
    flights: tuple[Flight, ...]
    reach: float


def jump_preview(settings: JumpSettings, request: dict) -> Preview:
    reply = call("jump_preview", settings.physics, request)
    flights = tuple(Flight.parse(bit, scenario) for bit, scenario in zip(SCENARIO_BITS, reply["scenarios"]))
    return Preview(flights, reply["edge_reach"])


# A stretch of a flight's path, as (x, z): `inside` until it falls past the viewed level's floor.
@dataclass(frozen=True)
class PathRun:
    phase: str
    inside: bool
    points: tuple[tuple[float, float], ...]


# Where a flight comes down on the viewed level. `supported` has a floor under
# it; `blocked` follows a supported landing the flight would have stopped on.
@dataclass(frozen=True)
class Glyph:
    phase: str
    point: tuple[float, float]
    damage: float
    supported: bool
    blocked: bool


# An outline on the viewed level, as line segments in (x, z): `kind` is
# capture, capture_steered, or range.
@dataclass(frozen=True)
class Region:
    kind: str
    segments: tuple[tuple[tuple[float, float], tuple[float, float]], ...]


@dataclass(frozen=True)
class LevelView:
    runs: tuple[PathRun, ...]
    glyphs: tuple[Glyph, ...]


# A path is `inside` from wherever it is at or above the viewed level's floor,
# however high it arcs, and outside once it has fallen past that floor: split
# exactly at the floor, where the landing is marked.
def path_runs(flight: Flight, level: int, level_height: float) -> list[PathRun]:
    height = level * level_height

    # Path heights are rounded to millimetres, so a floor is compared in them.
    def above(y):
        return round(y * 1000) >= round(height * 1000)

    runs = []
    points = None
    key = None

    def close():
        if points and any(max(abs(x - points[0][0]), abs(z - points[0][1])) > ON_LINE for x, z in points):
            runs.append(PathRun(*key, tuple(points)))

    for index in range(len(flight.path) - 1):
        # The hop itself is not travelled.
        if index == flight.hop:
            continue
        (ax, ay, az), (bx, by, bz) = flight.path[index], flight.path[index + 1]
        phase = AFTER_EXIT if flight.hop is not None and index > flight.hop else BEFORE_ENTRY
        stretches = [(above(ay), (ax, az), (bx, bz))]
        if above(ay) != above(by):
            along = min(max((ay - height) / (ay - by), 0.0), 1.0)
            at_floor = ax + (bx - ax) * along, az + (bz - az) * along
            stretches = [(above(ay), (ax, az), at_floor), (above(by), at_floor, (bx, bz))]
        for inside, start, stop in stretches:
            if key != (phase, inside):
                close()
                key, points = (phase, inside), [start]
            points.append(stop)
    close()
    return runs


def _covered_interval(a, b, polygon, keep_shared):
    low, high = 0.0, 1.0
    dx, dz = b[0] - a[0], b[1] - a[1]
    for index, p in enumerate(polygon):
        q = polygon[(index + 1) % len(polygon)]
        ex, ez = q[0] - p[0], q[1] - p[1]
        length = hypot(ex, ez)
        if length <= ON_LINE:
            continue
        before = (ex * (a[1] - p[1]) - ez * (a[0] - p[0])) / length
        after = (ex * (b[1] - p[1]) - ez * (b[0] - p[0])) / length
        if abs(before) <= ON_LINE and abs(after) <= ON_LINE:
            # Coincident outer edges are drawn once; opposite edges are inside the union.
            if keep_shared and ex * dx + ez * dz > 0:
                return None
            continue
        if before < 0 and after < 0:
            return None
        if before < 0:
            low = max(low, before / (before - after))
        elif after < 0:
            high = min(high, before / (before - after))
        if high <= low:
            return None
    return low, high


def outline_segments(polygons) -> list:
    polygons = list(dict.fromkeys(tuple(polygon) for polygon in polygons))
    for index, polygon in enumerate(polygons):
        area = sum(a[0] * b[1] - a[1] * b[0] for a, b in zip(polygon, polygon[1:] + polygon[:1]))
        if area < 0:
            polygons[index] = tuple(reversed(polygon))
    segments = []
    for index, polygon in enumerate(polygons):
        # Two points are one stroke, not a stroke there and back.
        for edge, a in enumerate(polygon if len(polygon) > 2 else polygon[:-1]):
            b = polygon[(edge + 1) % len(polygon)]
            visible = [(0.0, 1.0)]
            for other, cover in enumerate(polygons):
                if other == index or len(cover) < 3:
                    continue
                covered = _covered_interval(a, b, cover, keep_shared=other > index)
                if covered is None:
                    continue
                low, high = covered
                visible = [
                    interval
                    for start, stop in visible
                    for interval in ((start, min(stop, low)), (max(start, high), stop))
                    if interval[1] > interval[0]
                ]
                if not visible:
                    break
            dx, dz = b[0] - a[0], b[1] - a[1]
            for start, stop in visible:
                if (stop - start) * hypot(dx, dz) > ON_LINE:
                    segments.append(((a[0] + start * dx, a[1] + start * dz), (a[0] + stop * dx, a[1] + stop * dz)))
    return segments


def level_regions(flight: Flight, level: int) -> list[Region]:
    regions = []
    for kind, outlines in (
        ("capture", flight.capture),
        ("capture_steered", flight.capture_steered),
        ("range", {**flight.range, **flight.exit_range}),
    ):
        segments = outline_segments(outlines.get(level, ()))
        if segments:
            regions.append(Region(kind, tuple(segments)))
    return regions


def level_view(
    flight: Flight, level: int, settings: JumpSettings, footprints: FloorFootprints, reach: float = 0.0
) -> LevelView:
    glyphs = []
    landed = False
    for crossing in flight.crossings:
        supported = footprints.floor_under(crossing.level, *crossing.point, reach)
        if crossing.level == level:
            glyphs.append(Glyph(crossing.phase, crossing.point, crossing.damage, supported, blocked=landed))
        landed = landed or supported
    return LevelView(tuple(path_runs(flight, level, settings.level_height)), tuple(glyphs))


# What became of a flight's entry into portal 1: map_core's class, or
# `blocked` when a floor catches the flight before it gets there.
def entry_outcome(flight: Flight, footprints: FloorFootprints, reach: float = 0.0) -> str | None:
    blocked = flight.hop_time is not None and any(
        crossing.phase == BEFORE_ENTRY
        and crossing.time < flight.hop_time
        and footprints.floor_under(crossing.level, *crossing.point, reach)
        for crossing in flight.crossings
    )
    return "blocked" if blocked else flight.entry

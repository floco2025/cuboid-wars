"""The Jump Path tool without Qt: a takeoff on a tile edge, the request
map_core flies, and its reply arranged for one viewed level."""

from dataclasses import dataclass
from math import floor

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


def jump_preview(settings: JumpSettings, request: dict) -> tuple[Flight, ...]:
    scenarios = call("jump_preview", settings.physics, request)["scenarios"]
    return tuple(Flight.parse(bit, scenario) for bit, scenario in zip(SCENARIO_BITS, scenarios))


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


# The outline of polygons that tile one region. A capture region comes as a
# piece per quarter turn, cut along the diagonals through the place the
# portal is shot from; where two pieces meet on a diagonal the cut is inside
# the region, so only the stretches a single piece covers are kept.
def outline_segments(polygons, apex: tuple[float, float]) -> list:
    segments = []
    cuts = {1: [], -1: []}
    for polygon in polygons:
        # Two points are one stroke, not a stroke there and back.
        for index, a in enumerate(polygon if len(polygon) > 2 else polygon[:-1]):
            b = polygon[(index + 1) % len(polygon)]
            (ax, az), (bx, bz) = ((x - apex[0], z - apex[1]) for x, z in (a, b))
            for slope, cut in cuts.items():
                if abs(az - slope * ax) <= ON_LINE and abs(bz - slope * bx) <= ON_LINE:
                    cut.append((min(ax, bx), max(ax, bx)))
                    break
            else:
                segments.append((a, b))
    for slope, cut in cuts.items():
        ends = sorted({end for stretch in cut for end in stretch})
        for low, high in zip(ends, ends[1:]):
            middle = (low + high) / 2
            if high - low > ON_LINE and sum(a <= middle <= b for a, b in cut) % 2:
                segments.append(((apex[0] + low, apex[1] + slope * low), (apex[0] + high, apex[1] + slope * high)))
    return segments


# `apex` is where portal 1 is shot from, in metres.
def level_regions(flight: Flight, level: int, apex: tuple[float, float]) -> list[Region]:
    regions = []
    for kind, outlines in (
        ("capture", flight.capture),
        ("capture_steered", flight.capture_steered),
        ("range", {**flight.range, **flight.exit_range}),
    ):
        segments = outline_segments(outlines.get(level, ()), apex)
        if segments:
            regions.append(Region(kind, tuple(segments)))
    return regions


def level_view(flight: Flight, level: int, settings: JumpSettings, footprints: FloorFootprints) -> LevelView:
    glyphs = []
    landed = False
    for crossing in flight.crossings:
        supported = footprints.floor_under(crossing.level, *crossing.point)
        if crossing.level == level:
            glyphs.append(Glyph(crossing.phase, crossing.point, crossing.damage, supported, blocked=landed))
        landed = landed or supported
    return LevelView(tuple(path_runs(flight, level, settings.level_height)), tuple(glyphs))


# What became of a flight's entry into portal 1: map_core's class, or
# `blocked` when a floor catches the flight before it gets there.
def entry_outcome(flight: Flight, footprints: FloorFootprints) -> str | None:
    blocked = flight.hop_time is not None and any(
        crossing.phase == BEFORE_ENTRY
        and crossing.time < flight.hop_time
        and footprints.floor_under(crossing.level, *crossing.point)
        for crossing in flight.crossings
    )
    return "blocked" if blocked else flight.entry

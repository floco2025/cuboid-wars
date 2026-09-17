"""Open-space portal reach envelopes, independent of the editor widgets."""

from dataclasses import dataclass, replace
from itertools import pairwise
from math import atan2, ceil, floor, hypot, isfinite, pi, sqrt

from .catalogs import setting_number
from .floor_footprints import FloorFootprints
from .geometry import ramp_cells_on_level
from .jump_reach import ANTI_GRAVITY, BOTH, CHARACTER_TERMINAL_VELOCITY, NORMAL, SPEED, JumpSettings

EPS = 1e-9
# Match common/src/physics/characters/geometry.rs::character_movement_center.
CHARACTER_CONTACT_OFFSET = 0.01
# Keep aperture dimensions in sync with common/src/constants.rs.
PORTAL_HALF_WIDTH = 0.7
PORTAL_HALF_HEIGHT = 1.3
PORTAL_RIM_SCALE = 1.06
Vec3 = tuple[float, float, float]


def dot(a: Vec3, b: Vec3) -> float:
    return sum(x * y for x, y in zip(a, b))


@dataclass(frozen=True)
class PortalSettings:
    movement: JumpSettings
    body_height: float

    @property
    def center_height(self):
        return self.body_height / 2 + CHARACTER_CONTACT_OFFSET

    @classmethod
    def from_settings(cls, settings, source, *, gameplay, gameplay_source):
        movement = JumpSettings.from_settings(settings, source, gameplay=gameplay, gameplay_source=gameplay_source)
        return cls(movement, setting_number(gameplay, gameplay_source, "player.movement_collider.height"))

    def scenarios(self, running):
        m = self.movement
        speed = m.speed(running)
        return (
            (NORMAL, speed, m.gravity),
            (SPEED, speed * m.speed_multiplier, m.gravity),
            (ANTI_GRAVITY, speed, m.low_gravity),
            (BOTH, speed * m.speed_multiplier, m.low_gravity),
        )


@dataclass(frozen=True)
class PortalSurface:
    level: int
    col: int
    row: int
    face: str = "floor"
    turn: int = 0

    def placed_from(self, origin):
        if self.face != "floor":
            return self
        dx, dz = self.col - origin[1], self.row - origin[2]
        if dx == dz == 0:
            return replace(self, turn=0)
        # Match placement.rs::portal_placement_yaw, including Rust's ties away from zero.
        quarter_turns = atan2(dx, dz) / (pi / 2)
        snapped = floor(quarter_turns + 0.5) if quarter_turns >= 0 else ceil(quarter_turns - 0.5)
        return replace(self, turn=(2 - snapped) % 4)

    def frame(self, settings: PortalSettings):
        m = settings.movement
        size, y = m.cell_size, self.level * m.level_height
        if self.face == "floor":
            center = ((self.col + 0.5) * size, y, (self.row + 0.5) * size)
            normal = (0, 1, 0)
            up = ((0, 0, -1), (1, 0, 0), (0, 0, 1), (-1, 0, 0))[self.turn % 4]
        else:
            normal = {"north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0)}[self.face]
            horizontal = self.face in ("north", "south")
            center = (
                (self.col + (0.5 if horizontal else 0)) * size + normal[0] * m.wall_thickness / 2,
                y + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE,
                (self.row + (0 if horizontal else 0.5)) * size + normal[2] * m.wall_thickness / 2,
            )
            up = (0, 1, 0)
        right = (
            up[1] * normal[2] - up[2] * normal[1],
            up[2] * normal[0] - up[0] * normal[2],
            up[0] * normal[1] - up[1] * normal[0],
        )
        return PortalFrame(center, normal, up, right)


@dataclass(frozen=True)
class PortalFrame:
    center: Vec3
    normal: Vec3
    up: Vec3
    right: Vec3


def traverse_vector(entry: PortalFrame, exit: PortalFrame, velocity: Vec3) -> Vec3:
    # Match common/src/physics/portals/traversal.rs: a rotation, never a reflection.
    across, up, normal = dot(velocity, entry.right), dot(velocity, entry.up), dot(velocity, entry.normal)
    return tuple(exit.up[i] * up - exit.right[i] * across - exit.normal[i] * normal for i in range(3))


def descending_time(velocity: float, gravity: float, height: float) -> float | None:
    velocity = max(-CHARACTER_TERMINAL_VELOCITY, velocity)
    if gravity == 0:
        return height / velocity if velocity < -EPS and height < 0 else None
    discriminant = velocity * velocity - 2 * gravity * height
    if discriminant < -EPS:
        return None
    speed = sqrt(max(0, discriminant))
    if speed <= CHARACTER_TERMINAL_VELOCITY:
        return (velocity + speed) / gravity
    cap_time = (velocity + CHARACTER_TERMINAL_VELOCITY) / gravity
    cap_height = (velocity**2 - CHARACTER_TERMINAL_VELOCITY**2) / (2 * gravity)
    return cap_time + (cap_height - height) / CHARACTER_TERMINAL_VELOCITY


def crossings(velocity: float, gravity: float, height: float):
    down = descending_time(velocity, gravity, height)
    if down is not None and down > EPS:
        yield down, max(-CHARACTER_TERMINAL_VELOCITY, velocity - gravity * down)
    if velocity > 0:
        if gravity == 0:
            if height > 0:
                yield height / velocity, velocity
        elif 0 < height < velocity**2 / (2 * gravity) - EPS:
            time = (velocity - sqrt(velocity**2 - 2 * gravity * height)) / gravity
            yield time, velocity - gravity * time


def distance_to_rect(x, z, rect):
    x0, z0, x1, z1 = rect
    return hypot(max(x0 - x, 0, x - x1), max(z0 - z, 0, z - z1))


@dataclass(frozen=True)
class EntryState:
    time: float
    vertical_velocity: float
    speed: float
    gravity: float


def entry_states(settings, origin, surface, data, *, running, jumping, margin, footprints=None):
    if not isfinite(margin) or margin < 0:
        raise ValueError("Takeoff margin must be finite and nonnegative")
    m = settings.movement
    footprints = footprints or FloorFootprints(data, m.cell_size, m.wall_thickness)
    rectangles = footprints.rectangles(*origin)
    frame = surface.frame(settings)
    x, y, z = frame.center
    distance = min(distance_to_rect(x, z, rect) for rect in rectangles)
    height = y - (origin[0] * m.level_height + settings.center_height)
    velocity = m.jump_speed if jumping else 0
    margin = margin if jumping else 0
    result = {}
    for bit, speed, gravity in settings.scenarios(running):
        states = list(crossings(velocity, gravity, height))
        if gravity == velocity == 0 and abs(height) < EPS and surface.face != "floor":
            states = [(distance / speed + margin + EPS * 10, 0)]
        for time, vertical in states:
            if surface.face == "floor" and vertical >= -EPS:
                continue
            if time <= margin or distance > speed * (time - margin) + EPS:
                continue
            # At maximum range there is no time to steer around to a wall's front.
            if (
                surface.face != "floor"
                and abs(distance - speed * (time - margin)) < EPS
                and not any(
                    (min(max(x, a), c) - x) * frame.normal[0] + (min(max(z, b), d) - z) * frame.normal[2] > EPS
                    for a, b, c, d in rectangles
                )
            ):
                continue
            result.setdefault(bit, []).append(EntryState(time, vertical, speed, gravity))
    return result


def exit_motion(entry, exit, state):
    vertical = traverse_vector(entry, exit, (0, state.vertical_velocity, 0))
    x = traverse_vector(entry, exit, (1, 0, 0))[1]
    z = traverse_vector(entry, exit, (0, 0, 1))[1]
    extent = state.speed * hypot(x, z)
    low, high = vertical[1] - extent, vertical[1] + extent
    if exit.normal[1] == 1 and entry.normal[1] == 0:
        low = 0.0
    return (vertical[0], vertical[2]), low, high


def feasible_times(center, drift, speed, rect, start, end):
    """Intersect a growing, drifting steering disk with a rectangle, without time sampling."""
    breaks = [start, end]
    for p, v, low, high in zip(center, drift, rect[:2], rect[2:]):
        if abs(v) > EPS:
            breaks.extend(t for edge in (low, high) if start < (t := (edge - p) / v) < end)
    breaks = sorted(set(breaks))
    if len(breaks) == 1:
        if distance_to_rect(center[0] + drift[0] * start, center[1] + drift[1] * start, rect) <= speed * start + EPS:
            yield start, start
        return
    for left, right in pairwise(breaks):
        mid = (left + right) / 2
        a, b, c = -(speed**2), 0.0, 0.0
        for p, v, low, high in zip(center, drift, rect[:2], rect[2:]):
            value = p + v * mid
            edge = low if value < low else high if value > high else None
            if edge is not None:
                offset = p - edge
                a += v * v
                b += 2 * offset * v
                c += offset * offset
        cuts = [left, right]
        if abs(a) < EPS:
            if abs(b) > EPS:
                cuts.append(-c / b)
        elif (discriminant := b * b - 4 * a * c) >= -EPS:
            root = sqrt(max(0, discriminant))
            cuts.extend(((-b - root) / (2 * a), (-b + root) / (2 * a)))
        cuts = sorted({t for t in cuts if left <= t <= right})
        for t in cuts:
            if (a * t + b) * t + c <= EPS:
                yield t, t
        for lo, hi in pairwise(cuts):
            t = (lo + hi) / 2
            if (a * t + b) * t + c <= EPS:
                yield lo, hi


def landing_windows(settings, entry, exit, states, level):
    height = level * settings.movement.level_height - (exit.center[1] - settings.center_height)
    for state in states:
        # No accepted takeoff/entry in zero gravity produces a downward exit in this tool.
        if state.gravity == 0:
            continue
        drift, low, high = exit_motion(entry, exit, state)
        if height >= 0:
            low = max(low, sqrt(2 * state.gravity * height))
        if low > high + EPS:
            continue
        start = descending_time(low, state.gravity, height)
        end = descending_time(high, state.gravity, height)
        if start is None or end is None or end <= EPS:
            continue
        best = descending_time(0, state.gravity, height) if height < 0 else start
        yield state, drift, max(start, EPS), end, best, height


def calculate_landings(settings, entry_surface, exit_surface, states, data):
    m = settings.movement
    footprints = FloorFootprints(data, m.cell_size, m.wall_thickness)
    entry, exit = entry_surface.frame(settings), exit_surface.frame(settings)
    center = (exit.center[0], exit.center[2])
    result = {}
    for level in range(len(data["levels"])):
        excluded = ramp_cells_on_level(data["ramps"], level) | {
            (tile["col"], tile["row"]) for tile in data["levels"][level].get("terrain", [])
        }
        for bit, alternatives in states.items():
            for state, drift, start, end, best, height in landing_windows(settings, entry, exit, alternatives, level):
                radius = state.speed * end + m.wall_thickness
                bounds = [
                    (min(p + v * start, p + v * end) - radius, max(p + v * start, p + v * end) + radius)
                    for p, v in zip(center, drift)
                ]
                for row in range(
                    max(0, int(bounds[1][0] // m.cell_size)),
                    min(data["grid_rows"], int(bounds[1][1] // m.cell_size) + 1),
                ):
                    for col in range(
                        max(0, int(bounds[0][0] // m.cell_size)),
                        min(data["grid_cols"], int(bounds[0][1] // m.cell_size) + 1),
                    ):
                        if (col, row) in excluded:
                            continue
                        for rect in footprints.rectangles(level, col, row):
                            for lo, hi in feasible_times(center, drift, state.speed, rect, start, end):
                                time = min(max(best, lo), hi)
                                impact = min(
                                    CHARACTER_TERMINAL_VELOCITY, max(0, state.gravity * time / 2 - height / time)
                                )
                                damage = m.fall.damage_fraction(impact**2 / (2 * m.gravity), m.gravity, m.gravity)
                                landings = result.setdefault((level, col, row), {})
                                landings[bit] = min(landings.get(bit, 1.0), damage)
    return result

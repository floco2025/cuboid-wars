from dataclasses import dataclass
from math import ceil, isfinite, sqrt

from .floor_footprints import FloorFootprints


NORMAL = 1
SPEED = 2
ANTI_GRAVITY = 4
BOTH = 8


# Keep the cutoff and damage calculation in sync with server/src/players/falling.rs.
FALL_DAMAGE_EMIT_THRESHOLD = 1.0


@dataclass(frozen=True)
class FallSettings:
    safe_distance: float
    lethal_distance: float
    max_health: float

    def damage_fraction(self, drop: float, gravity: float, normal_gravity: float) -> float:
        distance = drop * (gravity / normal_gravity)
        fraction = min(1.0, max(0.0, (distance - self.safe_distance) / (self.lethal_distance - self.safe_distance)))
        return fraction if fraction * self.max_health >= FALL_DAMAGE_EMIT_THRESHOLD else 0.0


def _number(settings: dict, source: str, path: str, *, allow_zero: bool = False) -> float:
    value = settings
    for key in path.split("."):
        value = value.get(key) if isinstance(value, dict) else None
    if (
        type(value) not in (int, float)
        or not isfinite(value)
        or (value < 0 if allow_zero else value <= 0)
    ):
        requirement = "nonnegative" if allow_zero else "positive"
        raise ValueError(f"{source}: {path} must be a finite {requirement} number")
    return float(value)


@dataclass(frozen=True)
class JumpSettings:
    cell_size: float
    level_height: float
    jump_speed: float
    walk_speed: float
    run_speed: float
    speed_multiplier: float
    gravity: float
    low_gravity: float
    wall_thickness: float
    fall: FallSettings

    @classmethod
    def from_settings(cls, settings: dict, source: str, *, gameplay: dict, gameplay_source: str) -> "JumpSettings":
        def number(path: str, *, allow_zero: bool = False) -> float:
            return _number(settings, source, path, allow_zero=allow_zero)

        fall = FallSettings(
            number("player_fall.safe_distance", allow_zero=True),
            number("player_fall.lethal_distance"),
            _number(gameplay, gameplay_source, "combat.health.player.max"),
        )
        if fall.safe_distance >= fall.lethal_distance:
            raise ValueError(f"{source}: player_fall.safe_distance must be < player_fall.lethal_distance")
        return cls(
            number("geometry.grid_cell_size"),
            number("geometry.level_height"),
            number("movement.player.jump_speed"),
            number("movement.player.walk_speed"),
            number("movement.player.run_speed"),
            number("movement.player.speed_power_up"),
            number("movement.gravity"),
            number("movement.low_gravity", allow_zero=True),
            number("geometry.wall_thickness"),
            fall,
        )

    def speed(self, running: bool) -> float:
        return self.run_speed if running else self.walk_speed


def landing_time(jump_speed: float, gravity: float, height: float) -> float | None:
    if gravity == 0:
        return None
    discriminant = jump_speed * jump_speed - 2 * gravity * height
    if discriminant < 0:
        return None
    return (jump_speed + sqrt(discriminant)) / gravity


def calculate_reach(
    settings: JumpSettings,
    origin: tuple[int, int, int],
    data: dict,
    *,
    running: bool,
    margin: float,
) -> dict[tuple[int, int, int], dict[int, float]]:
    if not isfinite(margin) or margin < 0:
        raise ValueError("Takeoff margin must be finite and nonnegative")
    source_level, source_col, source_row = origin
    cols, rows = data["grid_cols"], data["grid_rows"]
    footprints = FloorFootprints(data, settings.cell_size, settings.wall_thickness)
    speed = settings.speed(running)
    scenarios = (
        (NORMAL, speed, settings.gravity),
        (SPEED, speed * settings.speed_multiplier, settings.gravity),
        (ANTI_GRAVITY, speed, settings.low_gravity),
        (BOTH, speed * settings.speed_multiplier, settings.low_gravity),
    )
    result = {}
    for level in range(len(data["levels"])):
        height = (level - source_level) * settings.level_height
        ranges = []
        for bit, velocity, gravity in scenarios:
            time = landing_time(settings.jump_speed, gravity, height)
            if time is not None and time >= margin:
                drop = max(0.0, settings.jump_speed * settings.jump_speed / (2 * gravity) - height)
                damage = settings.fall.damage_fraction(drop, gravity, settings.gravity)
                ranges.append((bit, velocity * (time - margin), damage))
        if not ranges:
            continue
        radius = ceil((max(distance for _, distance, _ in ranges) + settings.wall_thickness) / settings.cell_size) + 1
        for row in range(max(0, source_row - radius), min(rows, source_row + radius + 1)):
            for col in range(max(0, source_col - radius), min(cols, source_col + radius + 1)):
                if (level, col, row) == origin:
                    continue
                gap = footprints.distance(origin, (level, col, row))
                landings = {bit: damage for bit, distance, damage in ranges if gap <= distance + 1e-9}
                if landings:
                    result[level, col, row] = landings
    return result

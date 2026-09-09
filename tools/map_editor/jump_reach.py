from dataclasses import dataclass
from math import ceil, isfinite, sqrt

from .floor_footprints import FloorFootprints


NORMAL = 1
SPEED = 2
ANTI_GRAVITY = 4
BOTH = 8


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

    @classmethod
    def from_settings(cls, settings: dict, source: str) -> "JumpSettings":
        def number(path: str, *, allow_zero: bool = False) -> float:
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
) -> dict[tuple[int, int, int], int]:
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
                ranges.append((bit, velocity * (time - margin)))
        if not ranges:
            continue
        radius = ceil((max(distance for _, distance in ranges) + settings.wall_thickness) / settings.cell_size) + 1
        for row in range(max(0, source_row - radius), min(rows, source_row + radius + 1)):
            for col in range(max(0, source_col - radius), min(cols, source_col + radius + 1)):
                if (level, col, row) == origin:
                    continue
                gap = footprints.distance(origin, (level, col, row))
                mask = sum(bit for bit, distance in ranges if gap <= distance + 1e-9)
                if mask:
                    result[level, col, row] = mask
    return result

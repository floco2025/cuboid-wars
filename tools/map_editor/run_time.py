from dataclasses import dataclass
from math import hypot

from .catalogs import setting_number


@dataclass(frozen=True)
class RunSettings:
    cell_size: float
    move_speed: float
    speed_multiplier: float

    @classmethod
    def from_settings(cls, settings: dict, source: str) -> "RunSettings":
        return cls(
            setting_number(settings, source, "geometry.grid_cell_size"),
            setting_number(settings, source, "movement.player.move_speed"),
            setting_number(settings, source, "movement.player.move_speed_power_up"),
        )

    @property
    def boosted_speed(self) -> float:
        return self.move_speed * self.speed_multiplier

    # Straight-line travel at full speed; the run-up to that speed is not modelled.
    def seconds(self, origin: tuple[int, int], target: tuple[int, int]) -> tuple[float, float]:
        distance = hypot(target[0] - origin[0], target[1] - origin[1]) * self.cell_size
        return distance / self.move_speed, distance / self.boosted_speed

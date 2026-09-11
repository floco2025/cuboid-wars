from dataclasses import dataclass
from math import hypot

from .catalogs import setting_number


@dataclass(frozen=True)
class RunSettings:
    cell_size: float
    run_speed: float
    speed_multiplier: float

    @classmethod
    def from_settings(cls, settings: dict, source: str) -> "RunSettings":
        return cls(
            setting_number(settings, source, "geometry.grid_cell_size"),
            setting_number(settings, source, "movement.player.run_speed"),
            setting_number(settings, source, "movement.player.speed_power_up"),
        )

    @property
    def boosted_speed(self) -> float:
        return self.run_speed * self.speed_multiplier

    def seconds(self, origin: tuple[int, int], target: tuple[int, int]) -> tuple[float, float]:
        distance = hypot(target[0] - origin[0], target[1] - origin[1]) * self.cell_size
        return distance / self.run_speed, distance / self.boosted_speed

from dataclasses import dataclass

from .catalogs import setting_number
from .core import call


NORMAL = 1
SPEED = 2
ANTI_GRAVITY = 4
BOTH = 8

# The pickup combinations every preview flies, in the order map_core reports them.
SCENARIO_BITS = (NORMAL, SPEED, ANTI_GRAVITY, BOTH)

# The numbers a preview reads, checked here so a wrong type names its settings path;
# map_core then checks their relations (safe below lethal, and so on).
PHYSICS_NUMBERS = (
    ("network.server_hz", False),
    ("movement.gravity", False),
    ("movement.low_gravity", True),
    ("movement.player.move_speed", False),
    ("movement.player.move_speed_power_up", False),
    ("movement.player.move_speed_ladder", False),
    ("movement.player.jump_speed", False),
    ("movement.player.ground_acceleration", False),
    ("movement.player.ground_deceleration", False),
    ("movement.player.ground_lateral_deceleration", False),
    ("movement.player.air_acceleration", True),
    ("movement.player.air_deceleration", True),
    ("movement.player.air_lateral_deceleration", True),
    ("player_fall.safe_distance", True),
    ("player_fall.lethal_distance", False),
    ("combat.health.player.max", False),
    ("player.movement_collider.diameter", False),
    ("player.movement_collider.height", False),
    ("weapons.portals.size.width", False),
    ("weapons.portals.size.height", False),
    ("weapons.portals.funnel.capture_margin", True),
    ("weapons.portals.funnel.capture_growth", True),
)


@dataclass(frozen=True)
class JumpSettings:
    cell_size: float
    level_height: float
    wall_thickness: float
    # The movement block map_core validated: every physics number the preview uses.
    physics: dict

    @classmethod
    def from_settings(cls, settings: dict, source: str) -> "JumpSettings":
        def number(path: str) -> float:
            return setting_number(settings, source, path)

        for path, allow_zero in PHYSICS_NUMBERS:
            setting_number(settings, source, path, allow_zero=allow_zero)
        try:
            physics = call("preview_physics", settings)
        except ValueError as error:
            raise ValueError(f"{source}: {error}") from None
        return cls(
            number("geometry.grid_cell_size"),
            number("geometry.level_height"),
            number("geometry.wall_thickness"),
            physics,
        )

    @property
    def tick(self) -> float:
        return 1.0 / self.physics["server_hz"]

    @property
    def portal_half_width(self) -> float:
        return self.physics["portal_size"]["width"] / 2

    @property
    def portal_half_height(self) -> float:
        return self.physics["portal_size"]["height"] / 2

    def floor_height(self, level: int) -> float:
        return level * self.level_height

    # How long after walking off an edge the game still jumps as if from it.
    @property
    def coyote_secs(self) -> float:
        return self.physics["coyote_secs"]

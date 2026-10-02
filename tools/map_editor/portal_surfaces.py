"""Portal surfaces and face permissions in the active geometry; backing size is assumed."""

from dataclasses import dataclass, replace
from math import atan2, ceil, floor, pi

from .geometry import ramp_cells_on_level
from .jump_settings import JumpSettings
from .normalization import expand_face_materials

# The drawn rim is this much larger than the aperture, and it is the rim that rests on a wall's base.
PORTAL_RIM_SCALE = 1.06
WALL_NORMALS = {"north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0)}
# A floor portal's in-plane up by `turn`.
FLOOR_UPS = ((0, 0, -1), (1, 0, 0), (0, 0, 1), (-1, 0, 0))
Vec3 = tuple[float, float, float]


@dataclass(frozen=True)
class PortalFrame:
    center: Vec3
    normal: Vec3
    up: Vec3
    right: Vec3


# A floor portal lies anywhere on its cell, `offset` from the cell's corner in
# cells; a wall portal lies that far along its cell edge, at its middle
# unless told otherwise, with its rim on the wall's base.
@dataclass(frozen=True)
class PortalSurface:
    level: int
    col: int
    row: int
    face: str = "floor"
    turn: int = 0
    offset: tuple[float, float] = (0.5, 0.5)

    @classmethod
    def floor_at(cls, level: int, x: float, z: float) -> "PortalSurface":
        col, row = floor(x), floor(z)
        return cls(level, col, row, offset=(x - col, z - row))

    # The centre in grid units.
    @property
    def grid_center(self) -> tuple[float, float]:
        if self.face == "floor":
            return self.col + self.offset[0], self.row + self.offset[1]
        if self.face in ("north", "south"):
            return self.col + self.offset[0], self.row
        return self.col, self.row + self.offset[1]

    # A floor portal's long axis follows the quarter turn its shooter, a point in grid units, faces.
    def placed_from(self, shooter: tuple[float, float]) -> "PortalSurface":
        if self.face != "floor":
            return self
        x, z = self.grid_center
        # Match placement.rs::portal_placement_yaw, including Rust's ties away from zero.
        quarter_turns = atan2(x - shooter[0], z - shooter[1]) / (pi / 2)
        snapped = floor(quarter_turns + 0.5) if quarter_turns >= 0 else ceil(quarter_turns - 0.5)
        return replace(self, turn=(2 - snapped) % 4)

    def frame(self, settings: JumpSettings) -> PortalFrame:
        size, y = settings.cell_size, settings.floor_height(self.level)
        x, z = self.grid_center
        if self.face == "floor":
            center = (x * size, y, z * size)
            normal = (0, 1, 0)
            up = FLOOR_UPS[self.turn % 4]
        else:
            normal = WALL_NORMALS[self.face]
            center = (
                x * size + normal[0] * settings.wall_thickness / 2,
                y + settings.portal_half_height * PORTAL_RIM_SCALE,
                z * size + normal[2] * settings.wall_thickness / 2,
            )
            up = (0, 1, 0)
        right = (
            up[1] * normal[2] - up[2] * normal[1],
            up[2] * normal[0] - up[0] * normal[2],
            up[0] * normal[1] - up[1] * normal[0],
        )
        return PortalFrame(center, normal, up, right)

    # The surface as map_core places it: floor turns become the placement yaw the game snaps.
    def spec(self, settings: JumpSettings) -> dict:
        frame = self.frame(settings)
        yaw = (2 - self.turn % 4) * (pi / 2) if self.face == "floor" else 0.0
        return {"center": list(frame.center), "normal": list(frame.normal), "yaw": yaw}


@dataclass(frozen=True)
class SurfaceStatus:
    planned: bool
    reason: str = ""

    @property
    def available(self):
        return not self.reason

    @property
    def label(self):
        return self.reason or ("Planned compatible surface" if self.planned else "Existing compatible surface")


class PortalSurfaces:
    def __init__(self, data, textures):
        self.data, self.textures = data, textures
        self.floors = []
        self.terrain = []
        self.walls = []
        self.ramps = []
        for level, records in enumerate(data["levels"]):
            self.floors.append({(f["col"], f["row"]): f for f in records["floors"] + records["inaccessible_floors"]})
            self.terrain.append({(f["col"], f["row"]) for f in records.get("terrain", [])})
            edges = {}
            for wall in records["walls"]:
                horizontal = wall["r0"] == wall["r1"]
                if horizontal:
                    for col in range(min(wall["c0"], wall["c1"]), max(wall["c0"], wall["c1"])):
                        edges["h", col, wall["r0"]] = wall
                else:
                    for row in range(min(wall["r0"], wall["r1"]), max(wall["r0"], wall["r1"])):
                        edges["v", wall["c0"], row] = wall
            self.walls.append(edges)
            self.ramps.append(ramp_cells_on_level(data["ramps"], level))

    def allows(self, record, face):
        return self.textures.get(expand_face_materials(record)[face], False)

    # Judged by the cell under the centre; the aperture may reach past it.
    def status(self, surface):
        level, col, row = surface.level, surface.col, surface.row
        if surface.face == "floor":
            key = col, row
            record = self.floors[level].get(key)
            if key in self.ramps[level]:
                return SurfaceStatus(False, "Ramp surfaces are excluded")
            if key in self.terrain[level]:
                return SurfaceStatus(False, "Terrain surfaces are excluded")
            if record is not None and not self.allows(record, "top"):
                return SurfaceStatus(False, "Floor material does not allow portals")
        else:
            axis = "h" if surface.face in ("north", "south") else "v"
            record = self.walls[level].get((axis, col, row))
            if record is not None and not self.allows(record, surface.face):
                return SurfaceStatus(False, "Wall face material does not allow portals")
        return SurfaceStatus(record is None)

    # A floor portal is centred on the point itself, grid lines and corners included.
    def pick_floor(self, level, x, z):
        if 0 <= x < self.data["grid_cols"] and 0 <= z < self.data["grid_rows"]:
            return PortalSurface.floor_at(level, x, z)
        return None

    # A wall portal takes the nearest grid edge, on the face the point is on.
    def pick_wall(self, level, x, z):
        cols, rows = self.data["grid_cols"], self.data["grid_rows"]
        col, row = floor(x), floor(z)
        horizontal = PortalSurface(level, col, round(z), "north" if z < round(z) else "south")
        vertical = PortalSurface(level, round(x), row, "west" if x < round(x) else "east")
        candidates = [
            (abs(z - round(z)), 0 <= col < cols and 0 <= round(z) <= rows, horizontal),
            (abs(x - round(x)), 0 <= round(x) <= cols and 0 <= row < rows, vertical),
        ]
        return next((surface for _, inside, surface in sorted(candidates, key=lambda c: c[0]) if inside), None)


def portals_overlap(a, b, settings):
    frames = [surface.frame(settings) for surface in (a, b)]
    bounds = [
        [
            settings.portal_half_width * abs(frame.right[i])
            + settings.portal_half_height * abs(frame.up[i])
            + 0.05 * abs(frame.normal[i])
            for i in range(3)
        ]
        for frame in frames
    ]
    return all(abs(frames[0].center[i] - frames[1].center[i]) <= bounds[0][i] + bounds[1][i] for i in range(3))

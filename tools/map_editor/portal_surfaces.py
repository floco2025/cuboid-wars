"""Portal candidates and face permissions in the active geometry; backing size is assumed."""

from dataclasses import dataclass

from .floor_footprints import FloorFootprints
from .geometry import ramp_cells_on_level
from .normalization import expand_face_materials
from .portal_jump import PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PortalSurface


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
    def __init__(self, data, settings, textures):
        self.data, self.settings, self.textures = data, settings, textures
        m = settings.movement
        self.footprints = FloorFootprints(data, m.cell_size, m.wall_thickness)
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

    def candidates(self, level):
        cols, rows = self.data["grid_cols"], self.data["grid_rows"]
        for row in range(rows):
            for col in range(cols):
                yield PortalSurface(level, col, row)
        for row in range(rows + 1):
            for col in range(cols):
                for face in ("north", "south"):
                    yield PortalSurface(level, col, row, face)
        for row in range(rows):
            for col in range(cols + 1):
                for face in ("west", "east"):
                    yield PortalSurface(level, col, row, face)

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

    def pick(self, level, x, z, tolerance):
        cols, rows = self.data["grid_cols"], self.data["grid_rows"]
        col, row = int(x // 1), int(z // 1)
        dx, dz = abs(x - round(x)), abs(z - round(z))
        if min(dx, dz) <= min(tolerance, 0.25):
            if dz <= dx and 0 <= col < cols and 0 <= round(z) <= rows:
                return PortalSurface(level, col, round(z), "north" if z < round(z) else "south")
            if 0 <= round(x) <= cols and 0 <= row < rows:
                return PortalSurface(level, round(x), row, "west" if x < round(x) else "east")
        if 0 <= col < cols and 0 <= row < rows:
            return PortalSurface(level, col, row)
        return None


def portals_overlap(a, b, settings):
    frames = [surface.frame(settings) for surface in (a, b)]
    bounds = [
        [
            PORTAL_HALF_WIDTH * abs(frame.right[i])
            + PORTAL_HALF_HEIGHT * abs(frame.up[i])
            + 0.05 * abs(frame.normal[i])
            for i in range(3)
        ]
        for frame in frames
    ]
    return all(abs(frames[0].center[i] - frames[1].center[i]) <= bounds[0][i] + bounds[1][i] for i in range(3))

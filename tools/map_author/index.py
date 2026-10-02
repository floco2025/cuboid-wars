"""Per-level lookups over a layout, and the portal-readiness rules the views share."""

from __future__ import annotations

from dataclasses import dataclass
from math import hypot

from map_editor.geometry import ramp_cells_on_level, zone_rect
from map_editor.normalization import expand_face_materials

from .context import MapContext
from .edges import EdgeKey, cells_of_edge, edge_of_side, edges_of_record, front_cell

# PORTAL_PLATE_CLEARANCE in common/src/constants.rs: a plate this close to a floor portal blocks it.
PLATE_CLEARANCE = 1.2
Cell = tuple[int, int]


# A floor region a portal fits on: `axes` says along which grid axes two
# portalable cells line up with no wall between them.
@dataclass(frozen=True)
class FloorSurface:
    level: int
    cells: frozenset[Cell]
    axes: str
    plates: tuple[Cell, ...]

    @property
    def bounds(self) -> tuple[int, int, int, int]:
        cols = [c for c, _ in self.cells]
        rows = [r for _, r in self.cells]
        return min(cols), min(rows), max(cols) + 1, max(rows) + 1

    @property
    def ready(self) -> bool:
        return bool(self.axes) and not self.plates

    @property
    def reason(self) -> str:
        if self.plates:
            return "a pressure plate on the pad blocks portals within 1.2 m"
        if not self.axes:
            return "one cell alone: a floor portal needs two portalable cells side by side with no wall between"
        return ""


@dataclass(frozen=True)
class WallSurface:
    level: int
    key: EdgeKey
    face: str
    reason: str

    @property
    def ready(self) -> bool:
        return not self.reason

    @property
    def cell(self) -> Cell:
        return front_cell(self.key, self.face)


class MapIndex:
    def __init__(self, ctx: MapContext):
        self.ctx = ctx
        data = ctx.data
        self.count = len(data["levels"])
        self.floors, self.blocked, self.terrain, self.bridges = [], [], [], []
        self.walls, self.barriers, self.erasers, self.plates, self.items, self.checkpoints = [], [], [], [], [], []
        for index, level in enumerate(data["levels"]):
            self.floors.append({(f["col"], f["row"]): f for f in level["floors"]})
            self.blocked.append({(f["col"], f["row"]): f for f in level["inaccessible_floors"]})
            self.terrain.append({(f["col"], f["row"]): f for f in level.get("terrain", [])})
            self.bridges.append({(b["col"], b["row"]): b["field"] for b in level.get("light_bridges", [])})
            self.walls.append({key: wall for wall in level["walls"] for key in edges_of_record(wall)})
            self.barriers.append({key: b["field"] for b in level.get("barriers", []) for key in edges_of_record(b)})
            self.erasers.append({key for e in level.get("erasers", []) for key in edges_of_record(e)})
            self.plates.append(
                {(p["col"], p["row"]): p["switch"] for p in data["pressure_plates"] if p["level"] == index}
            )
            self.items.append({(i["col"], i["row"]): i for i in data["items"] if i["level"] == index})
            self.checkpoints.append([z for z in data["checkpoints"] if z["level"] == index])
        # A ramp is drawn on the storeys it climbs, not the one it arrives on.
        self.ramp_cells: list[dict[Cell, dict]] = [{} for _ in range(self.count)]
        for ramp in data["ramps"]:
            for index in range(ramp["lower_level"], min(ramp["lower_level"] + ramp["levels"], self.count)):
                for cell in ramp_cells_on_level([ramp], index):
                    self.ramp_cells[index][tuple(cell)] = ramp
        self.ladders: list[dict[EdgeKey, dict]] = [{} for _ in range(self.count)]
        for ladder in data["ladders"]:
            key = edge_of_side(ladder["col"], ladder["row"], ladder["side"])[0]
            for index in range(ladder["lower_level"], min(ladder["lower_level"] + ladder["levels"], self.count)):
                self.ladders[index][key] = ladder
        self._faces: dict[int, dict[str, str]] = {}

    def slab(self, level: int, cell: Cell) -> bool:
        return cell in self.floors[level] or cell in self.blocked[level] or cell in self.terrain[level]

    def faces(self, record: dict) -> dict[str, str]:
        key = id(record)
        if key not in self._faces:
            self._faces[key] = expand_face_materials(record)
        return self._faces[key]

    def portalable(self, record: dict, face: str) -> bool:
        return bool(self.ctx.textures.get(self.faces(record)[face], False))

    def portalable_top(self, level: int, cell: Cell) -> bool:
        record = self.floors[level].get(cell) or self.blocked[level].get(cell)
        return record is not None and cell not in self.ramp_cells[level] and self.portalable(record, "top")

    # Why a wall face takes no portal, or an empty string when it does: a
    # portal is taller than one storey, so the edge needs the same face one
    # level up with nothing standing in front of it there.
    def wall_reason(self, level: int, key: EdgeKey, face: str) -> str | None:
        record = self.walls[level].get(key)
        if record is None:
            return None
        if not self.portalable(record, face):
            return "face material is not portalable"
        if level + 1 >= self.count:
            return "top level: no storey above to stack a second wall section"
        above = self.walls[level + 1].get(key)
        if above is None:
            return "no wall section on the storey above (a portal needs two)"
        if not self.portalable(above, face):
            return "the upper wall section's face is not portalable"
        cell = front_cell(key, face)
        if self.slab(level + 1, cell) or cell in self.bridges[level + 1]:
            return "a floor in front on the storey above cuts the aperture"
        return ""

    def wall_surfaces(self, level: int) -> list[WallSurface]:
        surfaces = []
        for key in sorted(self.walls[level], key=lambda k: (k[0], k[2], k[1])):
            faces = ("north", "south") if key[0] == "h" else ("west", "east")
            for face in faces:
                reason = self.wall_reason(level, key, face)
                if reason is not None and reason != "face material is not portalable":
                    surfaces.append(WallSurface(level, key, face, reason))
        return surfaces

    def components(self, level: int, cells: set[Cell]) -> list[set[Cell]]:
        remaining = set(cells)
        groups = []
        while remaining:
            seed = min(remaining, key=lambda c: (c[1], c[0]))
            group, stack = set(), [seed]
            while stack:
                cell = stack.pop()
                if cell in group or cell not in remaining:
                    continue
                group.add(cell)
                col, row = cell
                stack.extend([(col + 1, row), (col - 1, row), (col, row + 1), (col, row - 1)])
            remaining -= group
            groups.append(group)
        return sorted(groups, key=lambda g: min((r, c) for c, r in g))

    def platforms(self, level: int) -> list[set[Cell]]:
        return self.components(level, set(self.floors[level]) | set(self.blocked[level]) | set(self.terrain[level]))

    def open_between(self, level: int, a: Cell, b: Cell) -> bool:
        key = ("v", max(a[0], b[0]), a[1]) if a[1] == b[1] else ("h", a[0], max(a[1], b[1]))
        return key not in self.walls[level]

    def floor_surfaces(self, level: int) -> list[FloorSurface]:
        candidates = set(self.floors[level]) | set(self.blocked[level])
        portalable = {cell for cell in candidates if self.portalable_top(level, cell)}
        surfaces = []
        for group in self.components(level, portalable):
            axes = ""
            if any((c + 1, r) in group and self.open_between(level, (c, r), (c + 1, r)) for c, r in group):
                axes += "x"
            if any((c, r + 1) in group and self.open_between(level, (c, r), (c, r + 1)) for c, r in group):
                axes += "z"
            plates = tuple(sorted(cell for cell in self.plates[level] if self._near_group(cell, group)))
            surfaces.append(FloorSurface(level, frozenset(group), axes, plates))
        return surfaces

    def _near_group(self, plate: Cell, group: set[Cell]) -> bool:
        cell = self.ctx.frame.cell
        px, pz = (plate[0] + 0.5) * cell, (plate[1] + 0.5) * cell
        for col, row in group:
            x = min(max(px, col * cell), (col + 1) * cell)
            z = min(max(pz, row * cell), (row + 1) * cell)
            if hypot(px - x, pz - z) < PLATE_CLEARANCE:
                return True
        return False

    def checkpoint_at(self, level: int, cell: Cell) -> int | None:
        for zone in self.checkpoints[level]:
            c0, r0, c1, r1 = zone_rect(zone)
            if c0 <= cell[0] < c1 and r0 <= cell[1] < r1:
                return zone["number"]
        return None

    # The cells a level's records touch, grown just enough that the plan can
    # draw every edge: a line is drawn between the cells on either side of it.
    def extent(self, level: int) -> tuple[int, int, int, int] | None:
        cells = set()
        for lookup in (self.floors, self.blocked, self.terrain, self.bridges, self.plates, self.items, self.ramp_cells):
            cells |= set(lookup[level])
        for zone in self.checkpoints[level]:
            c0, r0, c1, r1 = zone_rect(zone)
            cells |= {(c0, r0), (c1 - 1, r1 - 1)}
        edges = sorted({*self.walls[level], *self.barriers[level], *self.ladders[level], *self.erasers[level]})
        for axis, col, row in edges:
            if not cells:
                cells.add((col, row) if axis == "h" or col < self.ctx.frame.cols else (col - 1, row))
                continue
            c0, r0, c1, r1 = _bounds(cells)
            if axis == "h":
                cells.add((col, row - 1 if row >= r1 else row))
            else:
                cells.add((col - 1 if col >= c1 else col, row))
        return _bounds(cells) if cells else None


def _bounds(cells) -> tuple[int, int, int, int]:
    cols = [c for c, _ in cells]
    rows = [r for _, r in cells]
    return min(cols), min(rows), max(cols) + 1, max(rows) + 1

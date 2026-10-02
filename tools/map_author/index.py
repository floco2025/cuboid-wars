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
NOT_PORTALABLE = "face material is not portalable"
Cell = tuple[int, int]


# A floor region a portal fits on: `axes` says along which grid axes a block
# of `need` portalable cells, the portal's length by its width, lies with no
# wall inside it.
@dataclass(frozen=True)
class FloorSurface:
    level: int
    cells: frozenset[Cell]
    axes: str
    plates: tuple[Cell, ...]
    need: tuple[int, int]

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
            along, across = self.need
            return f"too small: a floor portal needs {along}x{across} portalable cells with no wall between"
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
        self._wall_surfaces: dict[int, list[WallSurface]] = {}

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

    # Why the wall sections stacked on one edge take no portal, or an empty
    # string when they do: a portal taller than a section needs the same face
    # on the storeys above with nothing standing in front of it there.
    def _stack_reason(self, level: int, key: EdgeKey, face: str) -> str | None:
        record = self.walls[level].get(key)
        if record is None:
            return None
        if not self.portalable(record, face):
            return NOT_PORTALABLE
        storeys = self.ctx.footprint.storeys
        cell = front_cell(key, face)
        for above in range(level + 1, level + storeys):
            if above >= self.count:
                return f"top level: no storey above to stack {storeys} wall sections"
            upper = self.walls[above].get(key)
            if upper is None:
                return f"no wall section on the storey above (a portal needs {storeys})"
            if not self.portalable(upper, face):
                return "the upper wall section's face is not portalable"
            if self.slab(above, cell) or cell in self.bridges[above]:
                return "a floor in front on the storey above cuts the aperture"
        return ""

    # Why a wall face takes no portal, or an empty string when it does. A
    # portal wider than a cell needs that many ready edges in a row.
    def wall_reason(self, level: int, key: EdgeKey, face: str) -> str | None:
        reason = self._stack_reason(level, key, face)
        across = self.ctx.footprint.across
        if reason is None or reason or across == 1:
            return reason
        axis, col, row = key
        dc, dr = (1, 0) if axis == "h" else (0, 1)
        run = 1
        for sign in (1, -1):
            c, r = col + sign * dc, row + sign * dr
            while run < across and self._stack_reason(level, (axis, c, r), face) == "":
                run += 1
                c, r = c + sign * dc, r + sign * dr
        if run < across:
            return f"{run} cell{'' if run == 1 else 's'} wide: a portal needs {across} such sections side by side"
        return ""

    def wall_surfaces(self, level: int) -> list[WallSurface]:
        if level not in self._wall_surfaces:
            surfaces = []
            for key in sorted(self.walls[level], key=lambda k: (k[0], k[2], k[1])):
                faces = ("north", "south") if key[0] == "h" else ("west", "east")
                for face in faces:
                    reason = self.wall_reason(level, key, face)
                    if reason is not None and reason != NOT_PORTALABLE:
                        surfaces.append(WallSurface(level, key, face, reason))
            self._wall_surfaces[level] = surfaces
        return self._wall_surfaces[level]

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
        along, across = self.ctx.footprint.along, self.ctx.footprint.across
        for group in self.components(level, portalable):
            axes = ""
            if self._block_fits(level, group, along, across):
                axes += "x"
            if self._block_fits(level, group, across, along):
                axes += "z"
            plates = tuple(sorted(cell for cell in self.plates[level] if self._near_group(cell, group)))
            surfaces.append(FloorSurface(level, frozenset(group), axes, plates, (along, across)))
        return surfaces

    # Whether some block of `cols` by `rows` cells of the group has no wall inside it.
    def _block_fits(self, level: int, group: set[Cell], cols: int, rows: int) -> bool:
        for c0, r0 in group:
            cells = [(c0 + i, r0 + j) for j in range(rows) for i in range(cols)]
            if (
                all(cell in group for cell in cells)
                and all(self.open_between(level, (c, r), (c + 1, r)) for c, r in cells if c + 1 < c0 + cols)
                and all(self.open_between(level, (c, r), (c, r + 1)) for c, r in cells if r + 1 < r0 + rows)
            ):
                return True
        return False

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

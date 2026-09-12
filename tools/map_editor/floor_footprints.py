from math import hypot

from .geometry import ramp_axis, ramp_rect


def slab_cells(data: dict) -> list[set[tuple[int, int]]]:
    return [
        {
            (tile["col"], tile["row"])
            for tile in level["floors"] + level["inaccessible_floors"]
            if 0 <= tile["col"] < data["grid_cols"] and 0 <= tile["row"] < data["grid_rows"]
        }
        for level in data["levels"]
    ]


def ramp_landing_edges(data: dict) -> list[set[tuple[str, int, int]]]:
    # Keep ramp exclusions in sync with server/src/map/ramps.rs::mark_high_end and definition/geometry.rs::compile_floors.
    edges = [set() for _ in data["levels"]]
    for ramp in data["ramps"]:
        upper = ramp["lower_level"] + 1
        axis = ramp_axis(ramp)
        if not 0 <= upper < len(edges):
            continue
        c0, r0, c1, r1 = ramp_rect(ramp)
        if axis in ("north", "south"):
            row = r1 if axis == "south" else r0
            edges[upper].update(("h", row, col) for col in range(c0, c1))
        else:
            col = c1 if axis == "east" else c0
            edges[upper].update(("v", row, col) for row in range(r0, r1))
    return edges


class FloorFootprints:
    def __init__(self, data: dict, cell_size: float, wall_thickness: float):
        self.cells = slab_cells(data)
        self.landings = ramp_landing_edges(data)
        self.cell_size = cell_size
        self.pad = wall_thickness / 2

    def rectangles(self, level: int, col: int, row: int, extra_cells=()) -> list[tuple[float, float, float, float]]:
        # Keep expansion changes in sync with server/src/map/floors.rs::emit_floor_tier, including corner fillers.
        occupied = self.cells[level]

        def has(dc, dr):
            return (col + dc, row + dr) in occupied or (col + dc, row + dr) in extra_cells

        west, east = has(-1, 0), has(1, 0)
        north, south = has(0, -1), has(0, 1)
        nw, ne, sw, se = has(-1, -1), has(1, -1), has(-1, 1), has(1, 1)
        landings = self.landings[level]
        landing_north = ("h", row, col) in landings
        landing_south = ("h", row + 1, col) in landings
        extend_north = not (north or nw or ne or landing_north)
        extend_south = not (south or sw or se or landing_south)
        x0, z0 = col * self.cell_size, row * self.cell_size
        x_end, z_end = (col + 1) * self.cell_size, (row + 1) * self.cell_size
        pad = self.pad
        x1 = x0 if west or ("v", row, col) in landings else x0 - pad
        x2 = x_end if east or ("v", row, col + 1) in landings else x_end + pad
        z1, z2 = z0 - pad if extend_north else z0, z_end + pad if extend_south else z_end
        rectangles = [(x1, z1, x2, z2)]
        if pad <= 0:
            return rectangles
        if not north and (nw or ne) and not landing_north:
            left, right = x0 + pad if nw else x1, x_end - pad if ne else x2
            if right > left:
                rectangles.append((left, z0 - pad, right, z0))
        if not south and (sw or se) and not landing_south:
            left, right = x0 + pad if sw else x1, x_end - pad if se else x2
            if right > left:
                rectangles.append((left, z_end, right, z_end + pad))
        return rectangles

    def distance(self, origin: tuple[int, int, int], target: tuple[int, int, int]) -> float:
        # Only these two hypothetical floors exist; other highlighted empty cells must not suppress their extensions.
        same_level = origin[0] == target[0]
        source = self.rectangles(*origin, extra_cells=(target[1:],) if same_level else ())
        destination = self.rectangles(*target, extra_cells=(origin[1:],) if same_level else ())
        return min(
            hypot(max(0, a[0] - b[2], b[0] - a[2]), max(0, a[1] - b[3], b[1] - a[3]))
            for a in source
            for b in destination
        )

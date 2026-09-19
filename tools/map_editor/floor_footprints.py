from math import hypot

from .core import call


def slab_cells(data: dict) -> list[set[tuple[int, int]]]:
    return [
        {
            (tile["col"], tile["row"])
            for tile in level["floors"] + level["inaccessible_floors"] + level.get("terrain", [])
            if 0 <= tile["col"] < data["grid_cols"] and 0 <= tile["row"] < data["grid_rows"]
        }
        for level in data["levels"]
    ]


def ramp_landing_edges(data: dict) -> list[set[tuple[str, int, int]]]:
    return [set(map(tuple, edges)) for edges in call("ramp_landing_edges", data)]


class FloorFootprints:
    def __init__(self, data: dict, cell_size: float, wall_thickness: float):
        self.cells = slab_cells(data)
        self.landings = ramp_landing_edges(data)
        self.cell_size = cell_size
        self.pad = wall_thickness / 2

    def rectangles(self, level: int, col: int, row: int, extra_cells=()) -> list[tuple[float, float, float, float]]:
        occupied = self.cells[level]
        neighbors = {
            name: (col + dc, row + dr) in occupied or (col + dc, row + dr) in extra_cells
            for name, dc, dr in (
                ("w", -1, 0),
                ("e", 1, 0),
                ("n", 0, -1),
                ("s", 0, 1),
                ("nw", -1, -1),
                ("ne", 1, -1),
                ("sw", -1, 1),
                ("se", 1, 1),
            )
        }
        edges = self.landings[level]
        landings = dict(
            n=("h", row, col) in edges,
            s=("h", row + 1, col) in edges,
            w=("v", row, col) in edges,
            e=("v", row, col + 1) in edges,
            nw=("v", row - 1, col) in edges,
            ne=("v", row - 1, col + 1) in edges,
            sw=("v", row + 1, col) in edges,
            se=("v", row + 1, col + 1) in edges,
        )
        bounds = (col * self.cell_size, row * self.cell_size, (col + 1) * self.cell_size, (row + 1) * self.cell_size)
        return list(map(tuple, call("floor_rectangles", bounds, self.pad, neighbors, landings)))

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

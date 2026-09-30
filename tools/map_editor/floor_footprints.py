from .core import call

# Rectangle edges this close to a point still hold it.
EDGE_SLACK = 1e-6


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
        self._rectangles = {}

    # The slab a floor on this cell has, or would have: its neighbours decide the extensions.
    def rectangles(self, level: int, col: int, row: int) -> list[tuple[float, float, float, float]]:
        key = level, col, row
        if key not in self._rectangles:
            self._rectangles[key] = self._slab(level, col, row)
        return self._rectangles[key]

    def _slab(self, level: int, col: int, row: int) -> list[tuple[float, float, float, float]]:
        occupied = self.cells[level]
        neighbors = {
            name: (col + dc, row + dr) in occupied
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

    # The point, in metres, on the slab's real edge `along` (0 to 1) the cell's `side`:
    # past the grid line where the slab extends over an exposed edge.
    def edge_point(self, level: int, col: int, row: int, side: str, along: float) -> tuple[float, float]:
        rectangles = self.rectangles(level, col, row)
        if side in "NS":
            x = (col + along) * self.cell_size
            spans = [r for r in rectangles if r[0] - EDGE_SLACK <= x <= r[2] + EDGE_SLACK] or rectangles
            return x, min(r[1] for r in spans) if side == "N" else max(r[3] for r in spans)
        z = (row + along) * self.cell_size
        spans = [r for r in rectangles if r[1] - EDGE_SLACK <= z <= r[3] + EDGE_SLACK] or rectangles
        return (min(r[0] for r in spans) if side == "W" else max(r[2] for r in spans)), z

    # The slab cell whose footprint holds a point in metres, or comes within
    # `reach` of it: its own cell's, or a neighbour's extension.
    def supporting_cell(self, level: int, x: float, z: float, reach: float = 0.0) -> tuple[int, int] | None:
        if not 0 <= level < len(self.cells):
            return None
        col, row = int(x // self.cell_size), int(z // self.cell_size)
        slack = reach + EDGE_SLACK
        for dc, dr in ((0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)):
            cell = col + dc, row + dr
            if cell in self.cells[level] and any(
                x0 - slack <= x <= x1 + slack and z0 - slack <= z <= z1 + slack
                for x0, z0, x1, z1 in self.rectangles(level, *cell)
            ):
                return cell
        return None

    def floor_under(self, level: int, x: float, z: float, reach: float = 0.0) -> bool:
        return self.supporting_cell(level, x, z, reach) is not None

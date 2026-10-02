"""The one grid <-> world conversion the authoring tools share."""

from __future__ import annotations

from dataclasses import dataclass
from math import floor

from map_editor.catalogs import load_map_settings, map_settings_path, setting_number

# A feet height this close to a floor top counts as standing on that level,
# the same tolerance the server's checkpoint rule uses.
LEVEL_TOLERANCE = 0.05


# Columns run along x and rows along z, the grid centred on the world origin;
# level L's floor top is at L * level_height. The editor's Jump Path works in
# grid-origin metres (col * cell) and the experiment runner in centred world
# metres, so every tool converts here and prints both.
@dataclass(frozen=True)
class GridFrame:
    cols: int
    rows: int
    cell: float
    level_height: float
    wall_thickness: float
    floor_thickness: float

    @classmethod
    def for_map(cls, name: str, data: dict) -> GridFrame:
        settings = load_map_settings(name)
        source = str(map_settings_path(name))
        cell, level_height, wall, floor_thickness = (
            setting_number(settings, source, f"geometry.{key}")
            for key in ("grid_cell_size", "level_height", "wall_thickness", "floor_thickness")
        )
        return cls(data["grid_cols"], data["grid_rows"], cell, level_height, wall, floor_thickness)

    @property
    def width(self) -> float:
        return self.cols * self.cell

    @property
    def depth(self) -> float:
        return self.rows * self.cell

    def level_y(self, level: int) -> float:
        return level * self.level_height

    # Grid units (a cell corner is an integer) to world metres.
    def grid_to_world(self, gx: float, gz: float) -> tuple[float, float]:
        return gx * self.cell - self.width / 2, gz * self.cell - self.depth / 2

    def world_to_grid(self, x: float, z: float) -> tuple[float, float]:
        return (x + self.width / 2) / self.cell, (z + self.depth / 2) / self.cell

    # Grid-origin metres, the Jump Path frame, to world metres.
    def metres_to_world(self, mx: float, mz: float) -> tuple[float, float]:
        return mx - self.width / 2, mz - self.depth / 2

    def world_to_metres(self, x: float, z: float) -> tuple[float, float]:
        return x + self.width / 2, z + self.depth / 2

    def cell_of_world(self, x: float, z: float) -> tuple[int, int]:
        gx, gz = self.world_to_grid(x, z)
        return floor(gx), floor(gz)

    def cell_center_world(self, col: int, row: int) -> tuple[float, float]:
        return self.grid_to_world(col + 0.5, row + 0.5)

    def level_of_y(self, y: float) -> int | None:
        level = round(y / self.level_height)
        return level if level >= 0 and abs(y - self.level_y(level)) <= LEVEL_TOLERANCE else None

    # "L3", or where between two floors a height lies.
    def describe_y(self, y: float) -> str:
        level = self.level_of_y(y)
        if level is not None:
            return f"L{level}"
        below = max(floor(y / self.level_height), 0)
        return f"between L{below}/L{below + 1} (y={y:.2f})"

    def world_bounds(self, c0: int, r0: int, c1: int, r1: int) -> tuple[float, float, float, float]:
        x0, z0 = self.grid_to_world(c0, r0)
        x1, z1 = self.grid_to_world(c1, r1)
        return x0, z0, x1, z1

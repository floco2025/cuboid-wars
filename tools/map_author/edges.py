"""Grid edges as the tools name them: a cell and the side its wall is on.

An edge key is ("h", col, row), the line z = row from x = col to col + 1, or
("v", col, row), the line x = col from z = row to row + 1. A wall face is
named by the way its normal points; the face on `side` of a cell looks back
into that cell."""

from __future__ import annotations

from map_editor.normalization import edge_key
from map_editor.portal_surfaces import PortalSurface

SIDES = ("N", "S", "E", "W")
FACE_INTO_CELL = {"N": "south", "S": "north", "E": "west", "W": "east"}
SIDE_OF_FACE = {face: side for side, face in FACE_INTO_CELL.items()}
# A step out of a cell over each side.
STEP = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}

EdgeKey = tuple[str, int, int]


def edge_of_side(col: int, row: int, side: str) -> tuple[EdgeKey, str]:
    if side == "N":
        return ("h", col, row), "south"
    if side == "S":
        return ("h", col, row + 1), "north"
    if side == "W":
        return ("v", col, row), "east"
    if side == "E":
        return ("v", col + 1, row), "west"
    raise ValueError(f"side must be one of {', '.join(SIDES)}, not {side!r}")


# The cell a face looks into.
def front_cell(key: EdgeKey, face: str) -> tuple[int, int]:
    axis, col, row = key
    if face == "north":
        return col, row - 1
    if face == "south":
        return col, row
    if face == "west":
        return col - 1, row
    return col, row


def surface_of_side(level: int, col: int, row: int, side: str) -> PortalSurface:
    (axis, edge_col, edge_row), face = edge_of_side(col, row, side)
    return PortalSurface(level, edge_col, edge_row, face)


# The cell and side a wall surface stands in front of.
def cell_side_of_surface(surface: PortalSurface) -> tuple[int, int, str]:
    axis = "h" if surface.face in ("north", "south") else "v"
    col, row = front_cell((axis, surface.col, surface.row), surface.face)
    return col, row, SIDE_OF_FACE[surface.face]


# Every unit edge a wall, barrier, or eraser record covers.
def edges_of_record(record: dict) -> list[EdgeKey]:
    c0, r0, c1, r1 = edge_key(record)
    if r0 == r1:
        return [("h", col, r0) for col in range(min(c0, c1), max(c0, c1))]
    return [("v", c0, row) for row in range(min(r0, r1), max(r0, r1))]


# Both cells an edge separates.
def cells_of_edge(key: EdgeKey) -> tuple[tuple[int, int], tuple[int, int]]:
    axis, col, row = key
    return ((col, row - 1), (col, row)) if axis == "h" else ((col - 1, row), (col, row))

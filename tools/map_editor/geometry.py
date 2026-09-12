"""Pure grid and spatial math for the editor canvas."""

from __future__ import annotations
import math


def wall_endpoints_for_cell_side(col: int, row: int, side: str) -> tuple[int, int, int, int]:
    """Return the canonical (c0, r0, c1, r1) of the wall on a cell's side."""
    if side == "N":
        c0, r0, c1, r1 = col, row, col + 1, row
    elif side == "S":
        c0, r0, c1, r1 = col, row + 1, col + 1, row + 1
    elif side == "W":
        c0, r0, c1, r1 = col, row, col, row + 1
    elif side == "E":
        c0, r0, c1, r1 = col + 1, row, col + 1, row + 1
    else:
        raise ValueError(f"unknown side {side!r}")
    return tuple(normalized_wall([c0, r0, c1, r1]))


def cell_side_from_click(col: int, row: int, px: float, py: float) -> str:
    """Return the cardinal side of cell (col, row) that the click (px, py) is
    closest to, in cell-unit coordinates."""
    distances = {
        "N": py - row,
        "S": (row + 1) - py,
        "W": px - col,
        "E": (col + 1) - px,
    }
    return min(distances, key=distances.get)


def normalized_wall(wall: list[int]) -> list[int]:
    c0, r0, c1, r1 = wall
    if (c1, r1) < (c0, r0):
        return [c1, r1, c0, r0]
    return [c0, r0, c1, r1]


# The eight resize handles of a zone, nw clockwise, in grid units.
def zone_handle_centers(zone: dict) -> list[tuple[float, float]]:
    c0, r0, c1, r1 = zone_rect(zone)
    mx, my = (c0 + c1) / 2, (r0 + r1) / 2
    return [(c0, r0), (mx, r0), (c1, r0), (c1, my), (c1, r1), (mx, r1), (c0, r1), (c0, my)]


def zone_rect(zone: dict) -> tuple[int, int, int, int]:
    return zone["cols"][0], zone["rows"][0], zone["cols"][1], zone["rows"][1]


def zone_intersects_rect(zone: dict, rect: tuple[int, int, int, int]) -> bool:
    return rects_overlap(zone_rect(zone), rect)


def zone_contains_cell(zone: dict, col: int, row: int) -> bool:
    c0, r0, c1, r1 = zone_rect(zone)
    return c0 <= col < c1 and r0 <= row < r1


def grid_point_in_bounds(col: int, row: int, cols: int, rows: int) -> bool:
    return 0 <= col <= cols and 0 <= row <= rows


def ramp_error(low: list[int], high: list[int], lower_level: int, cols: int, rows: int, level_count: int) -> str | None:
    if lower_level < 0 or lower_level + 1 >= level_count:
        return "lower_level must have an upper level"
    if not grid_point_in_bounds(low[0], low[1], cols, rows):
        return "low point is outside the grid-line bounds"
    if not grid_point_in_bounds(high[0], high[1], cols, rows):
        return "high point is outside the grid-line bounds"
    width = abs(high[0] - low[0])
    height = abs(high[1] - low[1])
    if width == 0 or height == 0:
        return "ramp must span a non-empty rectangular footprint"
    if width == height:
        return "ramp needs one clear longer axis"
    return None


def ramp_rect(ramp: dict) -> tuple[int, int, int, int]:
    low = ramp["low"]
    high = ramp["high"]
    return min(low[0], high[0]), min(low[1], high[1]), max(low[0], high[0]), max(low[1], high[1])


def ramp_cells(ramp: dict) -> set[tuple[int, int]]:
    c0, r0, c1, r1 = ramp_rect(ramp)
    return {(col, row) for row in range(r0, r1) for col in range(c0, c1)}


def ramp_cells_on_level(ramps: list[dict], level_idx: int) -> set[tuple[int, int]]:
    """Footprint cells of every ramp touching a level — a ramp occupies its
    cells on both the lower and the upper level."""
    cells: set[tuple[int, int]] = set()
    for ramp in ramps:
        if level_idx in (ramp["lower_level"], ramp["lower_level"] + 1):
            cells.update(ramp_cells(ramp))
    return cells


def ramp_axis(ramp: dict) -> str:
    low = ramp["low"]
    high = ramp["high"]
    dx = high[0] - low[0]
    dy = high[1] - low[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def opposite_direction(direction: str) -> str:
    return {
        "north": "south",
        "south": "north",
        "east": "west",
        "west": "east",
    }[direction]


# ============================================================================
# Drag / paint geometry helpers (cell rects, wall edges, ramp shapes)
# ============================================================================


def rect_from_cells(a: tuple[int, int], b: tuple[int, int]) -> tuple[int, int, int, int]:
    c0 = min(a[0], b[0])
    r0 = min(a[1], b[1])
    c1 = max(a[0], b[0]) + 1
    r1 = max(a[1], b[1]) + 1
    return c0, r0, c1, r1


def ramp_points_from_cells(start: tuple[int, int], end: tuple[int, int]) -> tuple[list[int], list[int]]:
    c0, r0, c1, r1 = rect_from_cells(start, end)
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) >= abs(dy):
        if dx >= 0:
            return [c0, r0], [c1, r1]
        return [c1, r0], [c0, r1]
    if dy >= 0:
        return [c0, r0], [c1, r1]
    return [c0, r1], [c1, r0]


def rects_overlap(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> bool:
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def wall_overlaps_rect(wall: list[int], rect: tuple[int, int, int, int]) -> bool:
    c0, r0, c1, r1 = rect
    wc0, wr0, wc1, wr1 = wall
    if wr0 == wr1:
        left = min(wc0, wc1)
        right = max(wc0, wc1)
        return r0 <= wr0 <= r1 and left < c1 and c0 < right
    top = min(wr0, wr1)
    bottom = max(wr0, wr1)
    return c0 <= wc0 <= c1 and top < r1 and r0 < bottom


def snapped_wall_end(start: tuple[int, int], current: tuple[int, int]) -> tuple[int, int]:
    dx = current[0] - start[0]
    dy = current[1] - start[1]
    if abs(dx) >= abs(dy):
        return current[0], start[1]
    return start[0], current[1]


def draw_direction(start: tuple[int, int], end: tuple[int, int]) -> str:
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if abs(dx) > abs(dy):
        return "east" if dx > 0 else "west"
    return "south" if dy > 0 else "north"


def wall_segments_between(start: tuple[int, int], end: tuple[int, int]) -> list[list[int]]:
    if start == end:
        return []
    c0, r0 = start
    c1, r1 = end
    edges = []
    if r0 == r1:
        step = 1 if c1 > c0 else -1
        for col in range(c0, c1, step):
            edges.append(normalized_wall([col, r0, col + step, r0]))
    elif c0 == c1:
        step = 1 if r1 > r0 else -1
        for row in range(r0, r1, step):
            edges.append(normalized_wall([c0, row, c0, row + step]))
    return edges


_OPPOSITE_SIDE = {"N": "S", "S": "N", "W": "E", "E": "W"}
_SIDE_NEIGHBOR = {"N": (0, -1), "S": (0, 1), "W": (-1, 0), "E": (1, 0)}


def ladder_anchor_from_click(col: int, row: int, side: str) -> tuple[int, int, str]:
    """The clicked cell is where the ladder physically stands (the climb
    side); the stored anchor is the cell across the clicked edge — its
    floors are the landings. Same edge, opposite side."""
    dc, dr = _SIDE_NEIGHBOR[side]
    return col + dc, row + dr, _OPPOSITE_SIDE[side]


def point_near_wall(px: float, py: float, wall: list[int], tolerance: float) -> bool:
    c0, r0, c1, r1 = wall
    if r0 == r1:
        return min(c0, c1) - tolerance <= px <= max(c0, c1) + tolerance and abs(py - r0) <= tolerance
    return min(r0, r1) - tolerance <= py <= max(r0, r1) + tolerance and abs(px - c0) <= tolerance


def zone_spans_level(zone: dict, level: int) -> bool:
    span = zone.get("levels", 1)
    return zone["level"] <= level < zone["level"] + (span if type(span) is int and span > 0 else 1)


def roam_slice_radius(zone: dict, level: int, level_height: float) -> float | None:
    distance = zone.get("roam_distance", 0.0)
    if type(distance) not in (int, float) or not math.isfinite(distance) or distance <= 0:
        return None
    if type(zone.get("levels", 1)) is not int or zone.get("levels", 1) < 1:
        return None
    low = zone["level"] * level_height
    high = (zone["level"] + zone.get("levels", 1)) * level_height
    y = level * level_height
    vertical = max(low - y, y - high, 0.0)
    if vertical > distance:
        return None
    return (distance * distance - vertical * vertical) ** 0.5

#!/usr/bin/env python3
"""ASCII preview of a building blueprint TOML.

Renders each level as a (2*rows+1) x (2*cols+1) grid where walls live on
even row/col indices and cell interiors live on odd indices.

  ┌───┬───┐
  │ · │ · │
  ├───┘   │
  │ ·   · │
  └───────┘

Usage:
  python3 tools/preview.py
  python3 tools/preview.py --level 2
  python3 tools/preview.py --file path/to/blueprint.toml
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BLUEPRINT = REPO_ROOT / "server" / "assets" / "buildings" / "default.toml"

# Cell interior glyphs.
FLOOR = "·"
NO_FLOOR = "█"
ATRIUM = "░"
RAMP_NORTH_UP = "▲"   # ramp surface ascending north (walk north to go up)
RAMP_SOUTH_UP = "▼"   # ramp surface ascending south
RAMP_BODY = "▒"       # ramp body — wedge from below pokes through this floor
RAMP_EXIT_NORTH = "↑" # cell where you step off a north-up ramp from the floor below
RAMP_EXIT_SOUTH = "↓" # cell where you step off a south-up ramp from the floor below

# Wall-edge glyphs (placed during edge pass; corners are computed at the end).
HORIZ = "─"
VERT = "│"
NONE = " "

# Corner glyph keyed by (north, south, east, west) booleans for which adjacent
# wall edges are present.
CORNERS = {
    (False, False, False, False): " ",
    (True,  False, False, False): "╵",
    (False, True,  False, False): "╷",
    (False, False, True,  False): "╶",
    (False, False, False, True ): "╴",
    (True,  True,  False, False): "│",
    (False, False, True,  True ): "─",
    (True,  False, True,  False): "└",
    (True,  False, False, True ): "┘",
    (False, True,  True,  False): "┌",
    (False, True,  False, True ): "┐",
    (True,  True,  True,  False): "├",
    (True,  True,  False, True ): "┤",
    (True,  False, True,  True ): "┴",
    (False, True,  True,  True ): "┬",
    (True,  True,  True,  True ): "┼",
}


def in_rect(rect, col, row):
    c0, r0, c_end, r_end = rect
    return c0 <= col < c_end and r0 <= row < r_end


def render_level(building, level_idx):
    level = building["levels"][level_idx]
    cols = building["grid_cols"]
    rows = building["grid_rows"]
    ramps = building.get("ramps", [])

    h = 2 * rows + 1
    w = 2 * cols + 1
    grid = [[NONE] * w for _ in range(h)]

    floor_rect = level["floor_rect"]
    atrium = level.get("atrium")

    # Cell interiors at (2*row+1, 2*col+1).
    for row in range(rows):
        for col in range(cols):
            ch = NO_FLOOR
            if in_rect(floor_rect, col, row):
                ch = FLOOR
            if atrium and in_rect(atrium, col, row):
                ch = ATRIUM
            grid[2 * row + 1][2 * col + 1] = ch

    # Ramps: surface at lower_level, body + landing at lower_level + 1.
    for ramp in ramps:
        lower = ramp["lower_level"]
        col = ramp["col"]
        row0 = ramp["row0"]
        direction = ramp["direction"]
        if level_idx == lower:
            ch = RAMP_NORTH_UP if direction == "north-up" else RAMP_SOUTH_UP
            for dr in (0, 1):
                grid[2 * (row0 + dr) + 1][2 * col + 1] = ch
        elif level_idx == lower + 1:
            for dr in (0, 1):
                grid[2 * (row0 + dr) + 1][2 * col + 1] = RAMP_BODY
            if direction == "north-up":
                exit_row, exit_ch = row0 - 1, RAMP_EXIT_NORTH
            else:
                exit_row, exit_ch = row0 + 2, RAMP_EXIT_SOUTH
            if 0 <= exit_row < rows:
                grid[2 * exit_row + 1][2 * col + 1] = exit_ch

    # Wall edges: perimeter + rooms. Doors clear the edge afterwards.
    rects = []
    perim = level.get("perimeter")
    if perim:
        rects.append(perim)
    rects.extend(level.get("rooms", []))

    for room in rects:
        c0, r0, c_end, r_end = room["rect"]
        for col in range(c0, c_end):
            grid[2 * r0][2 * col + 1] = HORIZ
            grid[2 * r_end][2 * col + 1] = HORIZ
        for row in range(r0, r_end):
            grid[2 * row + 1][2 * c0] = VERT
            grid[2 * row + 1][2 * c_end] = VERT

    for room in rects:
        c0, r0, c_end, r_end = room["rect"]
        for door in room.get("doors", []):
            side = door["side"]
            at = door["at"]
            if side == "N":
                grid[2 * r0][2 * at + 1] = NONE
            elif side == "S":
                grid[2 * r_end][2 * at + 1] = NONE
            elif side == "W":
                grid[2 * at + 1][2 * c0] = NONE
            elif side == "E":
                grid[2 * at + 1][2 * c_end] = NONE

    # Corners: choose glyph based on which neighboring edges have walls.
    for r in range(0, h, 2):
        for c in range(0, w, 2):
            n = r > 0 and grid[r - 1][c] == VERT
            s = r < h - 1 and grid[r + 1][c] == VERT
            e = c < w - 1 and grid[r][c + 1] == HORIZ
            w_ = c > 0 and grid[r][c - 1] == HORIZ
            grid[r][c] = CORNERS[(n, s, e, w_)]

    # Fill no-floor regions: wall-edge / corner positions surrounded by no-floor
    # cells become NO_FLOOR too, so the area outside the building renders as a
    # contiguous solid block.
    no_floor = [[not in_rect(floor_rect, c, r) for c in range(cols)] for r in range(rows)]

    def is_nf(cell_row, cell_col):
        if not (0 <= cell_row < rows and 0 <= cell_col < cols):
            return True
        return no_floor[cell_row][cell_col]

    for r in range(h):
        for c in range(w):
            if grid[r][c] != NONE:
                continue
            r_odd, c_odd = r % 2 == 1, c % 2 == 1
            if not r_odd and c_odd:
                # Horizontal wall edge: between cell rows r/2-1 and r/2.
                if is_nf(r // 2 - 1, (c - 1) // 2) and is_nf(r // 2, (c - 1) // 2):
                    grid[r][c] = NO_FLOOR
            elif r_odd and not c_odd:
                # Vertical wall edge: between cell cols c/2-1 and c/2.
                if is_nf((r - 1) // 2, c // 2 - 1) and is_nf((r - 1) // 2, c // 2):
                    grid[r][c] = NO_FLOOR
            elif not r_odd and not c_odd:
                # Corner: four surrounding cells.
                if (
                    is_nf(r // 2 - 1, c // 2 - 1)
                    and is_nf(r // 2 - 1, c // 2)
                    and is_nf(r // 2, c // 2 - 1)
                    and is_nf(r // 2, c // 2)
                ):
                    grid[r][c] = NO_FLOOR

    return grid


def widen(row):
    """Expand each cell interior / horizontal wall-edge to 3 chars so cell
    content is centered between its left and right wall (symmetric padding)
    and each cell renders roughly square in 2:1-aspect terminal fonts.
    Vertical walls and corners stay 1 char."""
    out = []
    for i, ch in enumerate(row):
        if i % 2 == 1:
            filler = HORIZ if ch == HORIZ else " "
            out.append(filler + ch + filler)
        else:
            out.append(ch)
    return "".join(out)


def render_blueprint(building, level_filter=None):
    out = []
    for idx, level in enumerate(building["levels"]):
        if level_filter is not None and idx != level_filter:
            continue
        out.append(f"=== Level {idx} ({level['name']}) ===")
        grid = render_level(building, idx)
        for row in grid:
            out.append(widen(row))
        out.append("")
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser(description="ASCII preview of a building blueprint TOML.")
    parser.add_argument("--file", type=Path, default=DEFAULT_BLUEPRINT, help="Path to the blueprint TOML.")
    parser.add_argument("--level", type=int, default=None, help="Render only this level index (0-based).")
    args = parser.parse_args()

    with args.file.open("rb") as f:
        data = tomllib.load(f)
    building = data["building"]

    if args.level is not None and not (0 <= args.level < len(building["levels"])):
        sys.exit(f"--level {args.level} out of range (0..{len(building['levels']) - 1})")

    print(render_blueprint(building, args.level))


if __name__ == "__main__":
    main()

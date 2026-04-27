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
RAMP_EAST_UP = "▶"    # ramp surface ascending east
RAMP_WEST_UP = "◀"    # ramp surface ascending west
RAMP_BODY = "▒"       # ramp body — wedge from below pokes through this floor
RAMP_EXIT_NORTH = "↑" # cell where you step off a north-up ramp from the floor below
RAMP_EXIT_SOUTH = "↓" # cell where you step off a south-up ramp from the floor below
RAMP_EXIT_EAST = "→"  # cell where you step off an east-up ramp from the floor below
RAMP_EXIT_WEST = "←"  # cell where you step off a west-up ramp from the floor below

# Cell types that render as a contiguous block: when adjacent cells match,
# the wall-edge / corner positions between them are filled with the same glyph
# instead of left as whitespace.
FILLABLE = {NO_FLOOR, ATRIUM, RAMP_BODY, RAMP_NORTH_UP, RAMP_SOUTH_UP, RAMP_EAST_UP, RAMP_WEST_UP}

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


def has_floor(level, col, row):
    return any(in_rect(rect, col, row) for rect in level["floors"]) and not any(
        in_rect(rect, col, row) for rect in level.get("voids", [])
    )


def render_level(building, level_idx):
    level = building["levels"][level_idx]
    cols = building["grid_cols"]
    rows = building["grid_rows"]
    ramps = building.get("ramps", [])

    h = 2 * rows + 1
    w = 2 * cols + 1
    grid = [[NONE] * w for _ in range(h)]

    # Cell interiors at (2*row+1, 2*col+1).
    for row in range(rows):
        for col in range(cols):
            ch = FLOOR if has_floor(level, col, row) else NO_FLOOR
            if any(in_rect(rect, col, row) for rect in level.get("voids", [])):
                ch = ATRIUM
            grid[2 * row + 1][2 * col + 1] = ch

    # Ramps: surface at lower_level, body + landing at lower_level + 1.
    for ramp in ramps:
        lower = ramp["lower_level"]
        c0, r0, c_end, r_end = ramp["rect"]
        up = ramp["up"]
        if level_idx == lower:
            ch = {
                "north": RAMP_NORTH_UP,
                "south": RAMP_SOUTH_UP,
                "east": RAMP_EAST_UP,
                "west": RAMP_WEST_UP,
            }[up]
            for row in range(r0, r_end):
                for col in range(c0, c_end):
                    grid[2 * row + 1][2 * col + 1] = ch
        elif level_idx == lower + 1:
            for row in range(r0, r_end):
                for col in range(c0, c_end):
                    grid[2 * row + 1][2 * col + 1] = RAMP_BODY
            if up == "north":
                exit_row = r0 - 1
                if 0 <= exit_row < rows:
                    for col in range(c0, c_end):
                        grid[2 * exit_row + 1][2 * col + 1] = RAMP_EXIT_NORTH
            elif up == "south":
                exit_row = r_end
                if 0 <= exit_row < rows:
                    for col in range(c0, c_end):
                        grid[2 * exit_row + 1][2 * col + 1] = RAMP_EXIT_SOUTH
            elif up == "east":
                exit_col = c_end
                if 0 <= exit_col < cols:
                    for row in range(r0, r_end):
                        grid[2 * row + 1][2 * exit_col + 1] = RAMP_EXIT_EAST
            elif up == "west":
                exit_col = c0 - 1
                if 0 <= exit_col < cols:
                    for row in range(r0, r_end):
                        grid[2 * row + 1][2 * exit_col + 1] = RAMP_EXIT_WEST

    # Wall edges: explicit grid-line segments. Doorways are gaps between
    # segments, so no separate clearing pass is needed.
    for wall in level.get("walls", []):
        c0, r0 = wall["from"]
        c1, r1 = wall["to"]
        if r0 == r1:
            for col in range(min(c0, c1), max(c0, c1)):
                grid[2 * r0][2 * col + 1] = HORIZ
        else:
            for row in range(min(r0, r1), max(r0, r1)):
                grid[2 * row + 1][2 * c0] = VERT

    # Corners: choose glyph based on which neighboring edges have walls.
    for r in range(0, h, 2):
        for c in range(0, w, 2):
            n = r > 0 and grid[r - 1][c] == VERT
            s = r < h - 1 and grid[r + 1][c] == VERT
            e = c < w - 1 and grid[r][c + 1] == HORIZ
            w_ = c > 0 and grid[r][c - 1] == HORIZ
            grid[r][c] = CORNERS[(n, s, e, w_)]

    # Fill regions of FILLABLE cell types: a wall-edge or corner position
    # surrounded by cells that all share the same fillable type takes that
    # type's glyph, so the region renders as a contiguous block (no-floor
    # outside the building, atrium void, ramp surface, ramp body).
    def cell_at(cell_row, cell_col):
        if not (0 <= cell_row < rows and 0 <= cell_col < cols):
            return NO_FLOOR
        return grid[2 * cell_row + 1][2 * cell_col + 1]

    for r in range(h):
        for c in range(w):
            if grid[r][c] != NONE:
                continue
            r_odd, c_odd = r % 2 == 1, c % 2 == 1
            if r_odd and c_odd:
                continue  # cell interior, already set
            if not r_odd and c_odd:
                touching = [(r // 2 - 1, (c - 1) // 2), (r // 2, (c - 1) // 2)]
            elif r_odd and not c_odd:
                touching = [((r - 1) // 2, c // 2 - 1), ((r - 1) // 2, c // 2)]
            else:
                touching = [
                    (r // 2 - 1, c // 2 - 1),
                    (r // 2 - 1, c // 2),
                    (r // 2, c // 2 - 1),
                    (r // 2, c // 2),
                ]
            types = {cell_at(rr, cc) for rr, cc in touching}
            if len(types) == 1:
                t = next(iter(types))
                if t in FILLABLE:
                    grid[r][c] = t

    return grid


def widen(row):
    """Expand each cell interior / horizontal wall-edge to 3 chars so cell
    content is centered between its left and right wall (symmetric padding)
    and each cell renders roughly square in 2:1-aspect terminal fonts.
    Vertical walls and corners stay 1 char."""
    out = []
    for i, ch in enumerate(row):
        if i % 2 == 1:
            if ch == HORIZ:
                filler = HORIZ
            elif ch in FILLABLE:
                filler = ch
            else:
                filler = " "
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
    parser.add_argument("-f", "--file", type=Path, default=DEFAULT_BLUEPRINT, help="Path to the blueprint TOML.")
    parser.add_argument("-l", "--level", type=int, default=None, help="Render only this level index (0-based).")
    args = parser.parse_args()

    with args.file.open("rb") as f:
        data = tomllib.load(f)
    building = data["building"]

    if args.level is not None and not (0 <= args.level < len(building["levels"])):
        sys.exit(f"--level {args.level} out of range (0..{len(building['levels']) - 1})")

    print(render_blueprint(building, args.level))


if __name__ == "__main__":
    main()
